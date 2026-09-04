pub mod catalog;
pub mod layered;
pub mod quad;
pub mod shade;
pub mod rasterize;
pub mod blockstate;

use self::layered::Layer;
use self::rasterize::Target;
use crate::diagnostics::Diagnostics;
use crate::direction::Quadrant;
use crate::resource::ResourceId;
use crate::resource::blockstate::{ModelPart, ModelState};
use crate::resource::cache::Caches;
use crate::resource::item::ItemModel;
use crate::resource::model::{DisplayContext, Geometry, Model, Transform};
use crate::resource::texture::sprite::{Sprite, missing_sprite};
use crate::resource::tint::TintSource;
use crate::util::FastHashMap;
use image::RgbaImage;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

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
    ) -> Result<RgbaImage, RenderError> {
        let state = self
            .caches
            .block_states
            .get(id)
            .ok_or_else(|| RenderError::NotPrimed(id.clone()))?;
        let query = blockstate::StateQuery::parse(props);
        let parts: Vec<ModelPart> = blockstate::parts(&state, &query).into_iter().cloned().collect();
        if parts.is_empty() {
            return Err(RenderError::NoVariant(id.clone(), props.to_owned()));
        }
        self.geometry(&parts, Self::tints_for_block(id), size, id).await
    }

    pub async fn render_item(&self, id: &ResourceId, size: u32) -> Result<RgbaImage, RenderError> {
        let item = self
            .caches
            .items
            .get(id)
            .ok_or_else(|| RenderError::NotPrimed(id.clone()))?;
        self.item_model(item.model.gui(), size, id).await
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
                self.geometry(std::slice::from_ref(&part), &resolved_tints, size, subject)
                    .await
            }
            Geometry::Empty => Err(RenderError::NoGeometry(subject.clone())),
        }
    }

    async fn item_model(
        &self,
        node: &ItemModel,
        size: u32,
        subject: &ResourceId,
    ) -> Result<RgbaImage, RenderError> {
        let mut nodes = Vec::new();
        draw_order(node, &mut nodes);
        let Some((first, rest)) = nodes.split_first() else {
            return Err(RenderError::NoGeometry(subject.clone()));
        };

        let base = self.leaf(first, size, subject).await?;
        if rest.is_empty() {
            return Ok(base);
        }

        let mut layers = vec![Layer { sprite: Arc::new(base), tint: u32::MAX }];
        for node in rest {
            if let Ok(image) = self.leaf(node, size, subject).await {
                layers.push(Layer { sprite: Arc::new(image), tint: u32::MAX });
            }
        }
        layered::composite(&layers, size).ok_or_else(|| RenderError::NoGeometry(subject.clone()))
    }

    async fn leaf(
        &self,
        node: &ItemModel,
        size: u32,
        subject: &ResourceId,
    ) -> Result<RgbaImage, RenderError> {
        match node {
            ItemModel::Model { model, tints } => self.model(model, tints, size, subject).await,
            // TODO: dump special models from mc
            ItemModel::Special { base } => self.model(base, &[], size, subject).await,
            _ => Err(RenderError::NoGeometry(subject.clone())),
        }
    }

    async fn geometry(
        &self,
        parts: &[ModelPart],
        tints: &[u32],
        size: u32,
        subject: &ResourceId,
    ) -> Result<RgbaImage, RenderError> {
        let mut tables = Vec::with_capacity(parts.len());
        for part in parts {
            tables.push(self.sprites_for(&part.model, subject).await);
        }

        let first = &parts[0].model;
        let display = first.display[DisplayContext::Gui as usize].unwrap_or(Transform::BLOCK_GUI);
        let shades = shade::shade_table(&display, first.gui_light);

        let mut quads = Vec::new();
        let mut elements = 0;
        for (part, textures) in parts.iter().zip(&tables) {
            if let Geometry::Cuboid(list) = &part.model.geometry {
                elements += list.len();
                quad::project(list, textures, tints, &part.state, &display, size, &mut quads);
            }
        }
        if quads.is_empty() {
            return Err(RenderError::NoGeometry(subject.clone()));
        }
        let depth = rasterize::needs_depth(parts.len(), elements, &quads);
        let mut target = Target::new(size, size, depth);
        rasterize::render(&mut target, &mut quads, &shades, depth);
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
            "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves" | "mangrove_leaves" | "vines" => &[0xFF48B518],
            "leaf_litter" => &[0xFF5C3C32],
            "water" | "water_cauldron" | "bubble_column" | "sugar_cane" => &[0xFFFFFFFF],
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

fn draw_order<'a>(node: &'a ItemModel, out: &mut Vec<&'a ItemModel>) {
    match node {
        ItemModel::Composite(layers) => layers.iter().for_each(|l| draw_order(l, out)),
        ItemModel::Model { .. } | ItemModel::Special { .. } => out.push(node),
        _ => {}
    }
}
