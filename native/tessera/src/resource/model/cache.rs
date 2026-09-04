use super::element::Face;
use super::{
    Element, GuiLight, Model, ModelError, RawModel, ShadeDirection, SlotContents, UnresolvedGeometry,
    UnresolvedModel, collapse,
};
use crate::diagnostics::Diagnostics;
use crate::direction::{Direction, Quadrant};
use crate::resource::ResourceId;
use crate::resource::resource_manager::ResourceManager;
use crate::resource::texture::TextureSlot;
use crate::resource::texture::cache::TextureCache;
use crate::resource::texture::face::{RawFace, UnresolvedTexture};
use crate::util::{FastDashMap, FastHashMap, FastHashSet, rayon_batch};
use foldhash::{HashMapExt, HashSetExt};
use std::borrow::Cow;
use std::sync::{Arc, OnceLock};
use ultraviolet::Vec3;

const MAX_DEPTH: usize = 64;

enum ChainStop {
    /// reached a model with no parent
    Complete,
    /// id is not in the unresolved cache
    Missing(ResourceId),
    /// parent cycle detected at id
    Cyclic(ResourceId),
    /// hit MAX_DEPTH
    TooDeep,
}

pub struct ModelCache {
    unresolved: FastDashMap<ResourceId, Arc<UnresolvedModel>>,
    resolved: FastDashMap<ResourceId, Arc<Model>>,
}

impl ModelCache {
    pub fn new() -> Self {
        let unresolved = FastDashMap::default();
        unresolved.insert(
            ResourceId::new_const("minecraft", "builtin/generated"),
            generated_item(),
        );
        Self { unresolved, resolved: FastDashMap::default() }
    }

    pub async fn prime(
        &self,
        rm: &ResourceManager,
        roots: impl IntoIterator<Item = ResourceId>,
        diag: &Diagnostics,
    ) {
        let mut seen = FastHashSet::new();
        let mut batch: Vec<ResourceId> = roots.into_iter().collect();

        while !batch.is_empty() {
            batch.retain(|id| seen.insert(id.clone()) && !self.unresolved.contains_key(id));
            if batch.is_empty() {
                break;
            }

            let paths: Vec<String> = batch.iter().map(ResourceId::model_path).collect();
            let bytes = rm.read_many(&paths).await;

            let parsed = rayon_batch(batch.iter().cloned().zip(bytes).collect(), |(id, bytes)| {
                let res = Self::parse_model(&id, bytes);
                (id, res)
            })
            .await;

            batch.clear();
            for (id, res) in parsed {
                match res {
                    Ok(model) => {
                        if let Some(parent) = &model.parent {
                            batch.push(parent.clone());
                        }
                        self.unresolved.insert(id, Arc::new(model));
                    }
                    Err(e) => diag.error(id, || e.to_string()),
                }
            }
        }
    }

    fn parse_model(
        id: &ResourceId,
        bytes: Option<Cow<'static, [u8]>>,
    ) -> Result<UnresolvedModel, ModelError> {
        let mut bytes = bytes.ok_or_else(|| ModelError::NotFound(id.clone()))?.into_owned();
        let raw: RawModel = simd_json::serde::from_slice(&mut bytes)
            .map_err(|e| ModelError::Parse(id.clone(), e.to_string()))?;
        UnresolvedModel::try_from(raw)
    }

    fn chain_cached(
        &self,
        start: ResourceId,
        chain: &mut Vec<Arc<UnresolvedModel>>,
        seen: &mut FastHashSet<ResourceId>,
    ) -> ChainStop {
        let mut next = Some(start);
        while let Some(current) = next {
            if seen.contains(&current) {
                return ChainStop::Cyclic(current);
            }
            if chain.len() >= MAX_DEPTH {
                return ChainStop::TooDeep;
            }
            let Some(model) = self.unresolved.get(&current).map(|m| m.clone()) else {
                return ChainStop::Missing(current);
            };

            next = model.parent.clone();
            chain.push(model);
            seen.insert(current);
        }
        ChainStop::Complete
    }

    pub async fn chain(
        &self,
        rm: &ResourceManager,
        id: &ResourceId,
        diag: &Diagnostics,
    ) -> Vec<Arc<UnresolvedModel>> {
        let mut chain = Vec::new();
        let mut seen = FastHashSet::new();
        let mut next = id.clone();

        loop {
            match self.chain_cached(next, &mut chain, &mut seen) {
                ChainStop::Complete => break,
                ChainStop::Cyclic(at) => {
                    diag.warn(id, || format!("cyclic model parent at {at}"));
                    break;
                }
                ChainStop::TooDeep => {
                    diag.warn(id, || "model parent chain too deep");
                    break;
                }

                // race condition is a feature Clueless
                // basically, if 2 models resolve to the same parent from 2 different tasks at the same
                // time, it would waste a parse. A OnceCell would deadlock.
                ChainStop::Missing(at) => match Self::parse_model(&at, rm.get_model_bytes(&at).await) {
                    Ok(model) => {
                        self.unresolved.insert(at.clone(), Arc::new(model));
                        next = at;
                    }
                    Err(err) => {
                        diag.error(id, || err.to_string());
                        if !chain.is_empty() {
                            chain.push(missing_unresolved().clone());
                        }
                        break;
                    }
                },
            }
        }
        chain
    }

    pub fn model_cached(&self, id: &ResourceId, diag: &Diagnostics) -> Option<Arc<Model>> {
        if let Some(m) = self.resolved.get(id) {
            return Some(m.clone());
        }
        let mut chain = Vec::new();
        let mut seen = FastHashSet::new();
        match self.chain_cached(id.clone(), &mut chain, &mut seen) {
            ChainStop::Missing(_) => return None,
            ChainStop::Complete => {}
            ChainStop::Cyclic(at) => diag.warn(id, || format!("cyclic model parent at {at}")),
            ChainStop::TooDeep => diag.warn(id, || "model parent chain too deep"),
        }
        if chain.is_empty() {
            return None;
        }
        let model = Arc::new(collapse(&chain, &id.to_string(), diag));
        self.resolved.insert(id.clone(), model.clone());
        Some(model)
    }

    pub async fn model(&self, rm: &ResourceManager, id: &ResourceId, diag: &Diagnostics) -> Arc<Model> {
        if let Some(m) = self.resolved.get(id) {
            return m.clone();
        }
        // Racy on purpose. Same as above
        let chain = self.chain(rm, id, diag).await;
        let model = if chain.is_empty() {
            missing_resolved().clone()
        } else {
            Arc::new(collapse(&chain, &id.to_string(), diag))
        };
        self.resolved.insert(id.clone(), model.clone());
        model
    }

    pub async fn prime_textures(&self, rm: &ResourceManager, textures: &TextureCache) {
        let mut seen = FastHashSet::new();
        for model in self.unresolved.iter() {
            for (_, slot) in model.textures.iter() {
                if let SlotContents::Value(texture) = slot {
                    seen.insert(texture.sprite.clone());
                }
            }
        }

        let seen = seen
            .into_iter()
            .filter_map(|id| id.parse::<ResourceId>().ok())
            .collect::<Vec<_>>();
        textures.prime(rm, &seen).await;
    }
}

impl Default for ModelCache {
    fn default() -> Self {
        Self::new()
    }
}

fn missing_unresolved() -> &'static Arc<UnresolvedModel> {
    static MISSING: OnceLock<Arc<UnresolvedModel>> = OnceLock::new();
    MISSING.get_or_init(|| {
        // TODO - rework structs to have str Cow property types instead of plain Strings
        #[rustfmt::skip]
        let faces = [
            Direction::North, Direction::East, Direction::South,
            Direction::West, Direction::Up, Direction::Down,
        ].map(|d| {
            let face = RawFace {
                texture: "#missingno".to_string(),
                uv: Some([0.0, 0.0, 16.0, 16.0]),
                cull_face: Some(d),
                rotation: Quadrant::R0,
                tint_index: None,
            };
            let missing_texture = UnresolvedTexture::from_raw(face, &Vec3::zero(), &Vec3::one(), d);
            Some(Face {
                uv: [missing_texture.from_x, missing_texture.from_y, missing_texture.to_x, missing_texture.to_y],
                rotation: missing_texture.rotation,
                tint_index: -1,
                texture: missing_texture.slot_key().to_owned(),
            })
        });

        let mut textures = FastHashMap::new();
        textures.insert(
            "missingno".to_string(),
            SlotContents::Value(TextureSlot {
                sprite: "minecraft:missingno".to_string(),
                force_translucent: false,
            }),
        );
        textures.insert(
            "particle".to_string(),
            SlotContents::Reference("missingno".to_string()),
        );

        Arc::new(UnresolvedModel {
            parent: None,
            textures,
            geometry: Some(UnresolvedGeometry::Cuboid(Arc::new(vec![Element {
                from: Vec3::zero(),
                to: Vec3::one(),
                faces,
                rotation: None,
                shade_direction: ShadeDirection::Actual,
                light_emission: 0,
            }]))),
            ambient_occlusion: None,
            gui_light: None,
            display: None,
        })
    })
}

fn missing_resolved() -> &'static Arc<Model> {
    static MISSING: OnceLock<Arc<Model>> = OnceLock::new();
    MISSING.get_or_init(|| {
        Arc::new(collapse(
            std::slice::from_ref(missing_unresolved()),
            "missing model",
            &Diagnostics::new(),
        ))
    })
}

fn generated_item() -> Arc<UnresolvedModel> {
    let mut textures = FastHashMap::new();
    textures.insert(
        "particle".to_string(),
        SlotContents::Reference("layer0".to_string()),
    );
    Arc::new(UnresolvedModel {
        parent: None,
        textures,
        geometry: Some(UnresolvedGeometry::GeneratedItem),
        gui_light: Some(GuiLight::Front),
        ambient_occlusion: None,
        display: None,
    })
}
