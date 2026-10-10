pub mod catalog;
pub mod layered;
pub mod quad;
pub mod shade;
pub mod rasterize;
pub mod blockstate;
pub mod capture;

use self::layered::Layer;
use self::quad::Rescale;
use self::rasterize::Target;
use crate::capture::lookup::{Capture, lookup_block};
use crate::diagnostics::Diagnostics;
use crate::direction::Quadrant;
use crate::resource::ResourceId;
use crate::resource::blockstate::{ModelPart, ModelState};
use crate::resource::cache::Caches;
use crate::resource::item::{ItemModel, Leaf, StaticContext};
use crate::resource::model::{DisplayContext, Geometry, GuiLight, Model, Transform};
use crate::resource::texture::TextureSlot;
use crate::resource::texture::sprite::{Sprite, missing_sprite};
use crate::resource::tint::TintSource;
use crate::scene::quad::{Affine, display_linear};
use crate::util::FastHashMap;
use image::RgbaImage;
use std::str::FromStr;
use std::sync::Arc;
use tessera_capture::model::RenderShape;
use thiserror::Error;
use crate::resource::model::cache::missing_resolved;

const FULL: ((f32, f32), (f32, f32)) = ((0.0, 0.0), (1.0, 1.0));

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("{0} has no geometry to render")]
    NoGeometry(ResourceId),
    #[error("No blockstate variant of {0} matches {1}")]
    NoVariant(ResourceId, String),
    #[error("{0} was not primed")]
    NotPrimed(ResourceId),
}

pub enum Framing {
    Gui,
    Fit { margin: f32 },
}

pub struct Renderer {
    caches: Caches,
}

impl Renderer {
    pub fn new(caches: Caches) -> Self {
        Self { caches }
    }

    pub async fn render_model(&self, id: &ResourceId, size: u32) -> Result<RgbaImage, RenderError> {
        let model = self
            .caches
            .models
            .model(&self.caches.resources, id, &self.diagnostics())
            .await;
        self.model(&model, &[], size, id).await
    }

    pub async fn render_blockstate(
        &self,
        id: &ResourceId,
        props: &str,
        size: u32,
        framing: Framing,
    ) -> Result<RgbaImage, RenderError> {
        let query = blockstate::StateQuery::parse(props);
        let (shape, captured) = match lookup_block(id, &query, self.diagnostics()) {
            Some((shape, capture)) => (shape, Some(capture)),
            None => (RenderShape::Model, None),
        };

        let parts: Vec<ModelPart> = if shape == RenderShape::Model {
            let state = self
                .caches
                .block_states
                .get(id)
                .ok_or_else(|| RenderError::NotPrimed(id.clone()))?;
            let parts: Vec<ModelPart> = blockstate::parts(&state, &query).into_iter().cloned().collect();
            if parts.is_empty() {
                return Err(RenderError::NoVariant(id.clone(), props.to_owned()));
            }
            parts
        } else {
            Vec::new()
        };
        self.geometry(&parts, captured, Self::tints_for_block(id), size, framing, id)
            .await
    }

    pub async fn render_item(&self, id: &ResourceId, size: u32) -> Result<RgbaImage, RenderError> {
        let item = self
            .caches
            .items
            .get(id)
            .ok_or_else(|| RenderError::NotPrimed(id.clone()))?;
        let mut leaves = Vec::new();
        item.model.leaves(&StaticContext, &mut leaves);
        if leaves.is_empty() {
            return Err(RenderError::NoGeometry(id.clone()));
        }

        if let Some(flat) = Self::flat_layers(&leaves) {
            let mut layers = Vec::new();
            for (slots, tints) in flat {
                let (resources, textures, grass) = (&self.caches.resources, &self.caches.textures, &self.caches.color_maps.grass);
                layers.extend(layered::layers(resources, textures, slots, tints, grass, &self.diagnostics()).await);
            }
            return layered::composite(&layers, size).ok_or_else(|| RenderError::NoGeometry(id.clone()));
        }

        self.item_scene(&leaves, size, id).await
    }

    fn flat_layers<'a>(leaves: &[Leaf<'a>]) -> Option<Vec<(&'a [TextureSlot], &'a [TintSource])>> {
        leaves
            .iter()
            .map(|leaf| match leaf {
                Leaf::Model { model, tints, .. } => {
                    // TODO transform on generated models (amethyst buds)
                    match &model.geometry {
                        Geometry::GeneratedItem(slots) => Some((&slots[..], *tints)),
                        _ => None,
                    }
                }
                _ => None, //TODO
            })
            .collect()
    }

    async fn item_scene(&self, leaves: &[Leaf<'_>], size: u32, subject: &ResourceId) -> Result<RgbaImage, RenderError> {
        let view =
            |display: &Option<Transform>, local: &Affine| Affine::gui(&display.unwrap_or(Transform::IDENTITY)).after(local);

        let mut quads = Vec::new();
        let mut elements = 0;
        for leaf in leaves {
            match leaf {
                Leaf::Model { model, tints, local } => {
                    let list = match &model.geometry {
                        Geometry::Cuboid(list) => list,
                        Geometry::Empty => continue,
                        Geometry::GeneratedItem(_) => {
                            // TODO
                            self.diagnostics()
                                .warn_keyed(subject, "mixed-composite", || "generated layer in a 3d model skipped");
                            continue;
                        }
                    };
                    let view = view(&model.display[DisplayContext::Gui as usize], local);
                    let shades = shade::shade_table(view.linear, model.gui_light);
                    let textures = self.sprites_for(model, subject).await;
                    let tints = shade::tint_table(tints, &self.caches.color_maps.grass);
                    elements += list.len();
                    quad::project(
                        list,
                        &textures,
                        &tints,
                        &ModelState::default(),
                        &view,
                        &shades,
                        size,
                        &mut quads,
                    );
                }
                Leaf::Special { base, local, capture: Some(capture) } => {
                    let view = view(&base.display[DisplayContext::Gui as usize], local);
                    let shades = shade::shade_table(view.linear, base.gui_light);
                    let mats = capture::materials(&self.caches, *capture, &[], subject).await;
                    elements += capture::project(
                        *capture,
                        &mats,
                        &view,
                        shade::lights(base.gui_light),
                        &shades,
                        size,
                        &mut quads,
                    );
                }
                Leaf::Special { base, local, .. } => {
                    let list = match &base.geometry {
                        Geometry::Cuboid(list) => list,
                        Geometry::Empty => continue,
                        Geometry::GeneratedItem(_) => {
                            // TODO
                            self.diagnostics()
                                .warn_keyed(subject, "mixed-composite", || "generated layer in a 3d model skipped");
                            continue;
                        }
                    };
                    let view = view(&base.display[DisplayContext::Gui as usize], local);
                    let shades = shade::shade_table(view.linear, base.gui_light);
                    let textures = self.sprites_for(base, subject).await;
                    elements += list.len();
                    quad::project(list, &textures, &[], &ModelState::default(), &view, &shades, size, &mut quads);
                }
                Leaf::Missing => {
                    let model = missing_resolved();
                    let Geometry::Cuboid(list) = &model.geometry else {
                        unreachable!()
                    };
                    let view = view(&model.display[DisplayContext::Gui as usize], &Affine::identity());
                    let shades = shade::shade_table(view.linear, model.gui_light);
                    let textures = self.sprites_for(model, subject).await;
                    elements += list.len();
                    quad::project(list, &textures, &[], &ModelState::default(), &view, &shades, size, &mut quads);
                }
            }
        }

        if quads.is_empty() {
            return Err(RenderError::NoGeometry(subject.clone()));
        }
        let depth = rasterize::needs_depth(leaves.len(), elements, &quads);
        let mut target = Target::new(size, size, depth);
        rasterize::render(&mut target, &mut quads, depth);
        Ok(target.into_image())
    }

    async fn model(
        &self,
        model: &Arc<Model>,
        tints: &[TintSource],
        size: u32,
        subject: &ResourceId,
    ) -> Result<RgbaImage, RenderError> {
        match &model.geometry {
            Geometry::GeneratedItem(slots) => layered::render(
                &self.caches.resources,
                &self.caches.textures,
                slots,
                tints,
                &self.caches.color_maps.grass,
                size,
                &self.diagnostics(),
            )
            .await
            .ok_or_else(|| RenderError::NoGeometry(subject.clone())),
            Geometry::Cuboid(_) => {
                let part = ModelPart { model: model.clone(), state: ModelState::default() };
                let resolved_tints = shade::tint_table(tints, &self.caches.color_maps.grass);
                self.geometry(
                    std::slice::from_ref(&part),
                    None,
                    &resolved_tints,
                    size,
                    Framing::Gui,
                    subject,
                )
                .await
            }
            Geometry::Empty => Err(RenderError::NoGeometry(subject.clone())),
        }
    }

    async fn geometry(
        &self,
        parts: &[ModelPart],
        captured: Option<Capture<'static>>,
        tints: &[u32],
        size: u32,
        framing: Framing,
        subject: &ResourceId,
    ) -> Result<RgbaImage, RenderError> {
        let mut tables = Vec::with_capacity(parts.len());
        for part in parts {
            tables.push(self.sprites_for(&part.model, subject).await);
        }
        let captured_materials = match captured {
            Some(captured) => capture::materials(&self.caches, captured, tints, subject).await,
            None => Vec::new(),
        };
        let (display, light) = parts.first().map_or((Transform::BLOCK_GUI, GuiLight::Side), |part| {
            (
                part.model.display[DisplayContext::Gui as usize].unwrap_or(Transform::BLOCK_GUI),
                part.model.gui_light,
            )
        });
        let view = Affine::gui(&display);
        let shades = shade::shade_table(view.linear, light);

        let mut quads = Vec::new();
        let mut elements = 0;
        for (part, textures) in parts.iter().zip(&tables) {
            if let Geometry::Cuboid(list) = &part.model.geometry {
                elements += list.len();
                quad::project(list, textures, tints, &part.state, &view, &shades, size, &mut quads);
            }
        }
        if let Some(cap) = captured {
            elements += capture::project(
                cap,
                &captured_materials,
                &view,
                shade::lights(light),
                &shades,
                size,
                &mut quads,
            );
        }
        if let Framing::Fit { margin } = framing
            && !quads.is_empty()
        {
            let rescale = Rescale::fit(&quads, size as f32, margin);
            quads.iter_mut().for_each(|quad| quad.rescale(rescale));
        }
        if quads.is_empty() {
            return Err(RenderError::NoGeometry(subject.clone()));
        }
        let depth = rasterize::needs_depth(parts.len(), elements, &quads);
        let mut target = Target::new(size, size, depth);
        rasterize::render(&mut target, &mut quads, depth);
        Ok(target.into_image())
    }

    async fn sprites_for(&self, model: &Model, subject: &ResourceId) -> FastHashMap<String, Arc<Sprite>> {
        let mut out = FastHashMap::default();
        for (key, slot) in model.textures.iter() {
            let sprite = match ResourceId::from_str(&slot.sprite) {
                Err(_) => {
                    self.diagnostics().error(subject, || {
                        format!("Slot \"{key}\" is not a valid resource id: {}", slot.sprite)
                    });
                    missing_sprite().clone()
                }
                Ok(id) => {
                    let loaded = self
                        .caches
                        .textures
                        .sprite(&self.caches.resources, &id, Quadrant::R0, FULL.0, FULL.1)
                        .await;
                    match loaded {
                        Ok(sprite) => sprite,
                        Err(err) => {
                            self.diagnostics().error(subject, || format!("Slot \"{key}\": {err}"));
                            missing_sprite().clone()
                        }
                    }
                }
            };
            out.insert(key.clone(), sprite);
        }
        out
    }

    /// [ref](https://mcsrc.dev/2/26.2/net/minecraft/client/color/block/BlockColors)
    /// [ref](https://mcsrc.dev/2/26.2/net/minecraft/client/color/block/BlockTintSources)
    #[rustfmt::skip]
    fn tints_for_block(block: &ResourceId) -> &'static [u32] {
        if block.namespace != "minecraft" {
            return &[];
        }
        const GRASS: u32 = 0xFF7CBD6B;

        match block.path.as_ref() {
            "grass_block" | "short_grass" | "fern" | "potted_fern" | "bush" | "tall_grass" | "large_fern" => &[GRASS],
            "pink_petals" | "wildflowers" => &[0xFFFFFFFF, GRASS],
            "spruce_leaves" => &[0xFF619961],
            "birch_leaves" => &[0xFF80A755],
            "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves" | "mangrove_leaves" | "vine" => &[0xFF48B518],
            "leaf_litter" => &[0xFF5C3C32],
            "water" | "water_cauldron" | "bubble_column" => &[0xFF3F76E4],
            "sugar_cane" => &[0xFFFFFFFF],
            "redstone_wire" => &[0xFF4C0000],
            "attached_pumpkin_stem" | "attached_melon_stem" => &[0xFFE0C71C],
            "pumpkin_stem" | "melon_stem" => &[0xFF00FF00],
            "lily_pad" => &[0xFF71C35C],
            _ => &[],
        }
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.caches.diagnostics()
    }
}
