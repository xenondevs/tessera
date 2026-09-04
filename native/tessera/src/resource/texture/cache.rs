use super::atlas::{SpriteIndex, SpriteRecipe};
use super::mcmeta::RawTextureMeta;
use super::palette::{Palette, PaletteMapping, PaletteSizeMismatch};
use super::sprite::Sprite;
use crate::diagnostics::Diagnostics;
use crate::direction::Quadrant;
use crate::resource::ResourceId;
use crate::resource::resource_manager::ResourceManager;
use crate::util;
use crate::util::{FastDashMap, FastHashMap, FastHashSet, rayon_batch};
use image::{Rgba, RgbaImage};
use std::borrow::Cow;
use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use tokio::sync::OnceCell;

#[derive(Clone, Debug, Error)]
pub enum TextureError {
    #[error("No known pack provides texture {0}")]
    NotFound(ResourceId),
    #[error("Failed to decode texture {0}: {1}")]
    Decode(ResourceId, String),
    #[error("Texture {0} has a zero dimension")]
    Empty(ResourceId),
    #[error("Cannot build a palette mapping for {0}: {1}")]
    Palette(ResourceId, PaletteSizeMismatch),
    #[error("Unstitch region for {sprite} does not fit inside {base}")]
    InvalidRegion { sprite: ResourceId, base: ResourceId },
    #[error("Animation frame size does not evenly divide texture {0}")]
    InvalidFrameSize(ResourceId),
}

pub enum MappingError {
    Size(PaletteSizeMismatch),
    Load(TextureError),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureOptions {
    id: ResourceId,
    rotation: Quadrant,
    uv_bits: [u32; 4],
}

impl TextureOptions {
    fn new(id: ResourceId, rotation: Quadrant, from: (f32, f32), to: (f32, f32)) -> TextureOptions {
        Self {
            id,
            rotation,
            uv_bits: [from.0.to_bits(), from.1.to_bits(), to.0.to_bits(), to.1.to_bits()],
        }
    }
}

type Cell<T> = Arc<OnceCell<T>>;

/// A decoded artifact, or why it failed.
pub type Loaded<T> = Result<T, TextureError>;

/// A batch of decoded artifacts to be processed and shared across rayon workers
type Batch<K, T, E = TextureError> = Arc<FastHashMap<K, Result<T, E>>>;

/// (subject, message)
type QueuedWarning = Option<(String, String)>;

/// A sprite and the recipe that builds it. None means plain file.
type Plan = (ResourceId, Option<SpriteRecipe>);

pub struct TextureCache {
    raw: FastDashMap<ResourceId, Cell<Loaded<Arc<RgbaImage>>>>,
    meta: FastDashMap<ResourceId, Cell<Arc<RawTextureMeta>>>,
    rendered: FastDashMap<TextureOptions, Cell<Loaded<Arc<RgbaImage>>>>,
    sprites: FastDashMap<TextureOptions, Cell<Loaded<Arc<Sprite>>>>,
    index: OnceCell<Arc<SpriteIndex>>,
    substitute_missing: bool,
}

impl TextureCache {
    pub fn new(substitute_missing: bool) -> TextureCache {
        Self {
            raw: FastDashMap::default(),
            meta: FastDashMap::default(),
            rendered: FastDashMap::default(),
            sprites: FastDashMap::default(),
            index: OnceCell::new(),
            substitute_missing,
        }
    }

    // TODO check if on demand is even needed or if priming really discovers everything
    pub async fn prime(&self, rm: &ResourceManager, ids: &[ResourceId]) {
        let index = self.index(rm).await;

        let mut recipes: Vec<(ResourceId, Option<SpriteRecipe>)> = Vec::with_capacity(ids.len());
        let mut wanted_meta: FastHashSet<ResourceId> = FastHashSet::default();
        let mut wanted_images: FastHashSet<ResourceId> = FastHashSet::default();
        let mut wanted_palettes: FastHashSet<ResourceId> = FastHashSet::default();

        for id in ids {
            if self.raw.contains_key(id) {
                continue;
            }

            let recipe = index.recipe(id);

            match &recipe {
                None => {
                    wanted_meta.insert(id.clone());
                    wanted_images.insert(id.clone());
                }
                Some(SpriteRecipe::File(src)) => {
                    wanted_meta.insert(src.clone());
                    wanted_images.insert(src.clone());
                }
                Some(SpriteRecipe::Region { base, .. }) => {
                    wanted_images.insert(base.clone());
                }
                Some(SpriteRecipe::Palettized { base, key, palette }) => {
                    wanted_images.insert(base.clone());
                    wanted_palettes.insert(key.clone());
                    wanted_palettes.insert(palette.clone());
                }
            };
            recipes.push((id.clone(), recipe.cloned()));
        }

        if recipes.is_empty() {
            return;
        }

        let image_ids: Vec<ResourceId> = wanted_images.into_iter().collect();
        let palette_ids: Vec<ResourceId> = wanted_palettes.into_iter().collect();

        let meta_fut = self.prime_meta(rm, wanted_meta);
        let sprites_fut = async {
            let (images, palettes) = Self::decode_sources(rm, image_ids, palette_ids).await;
            let mappings = Self::build_mappings(&recipes, palettes.clone()).await;
            Self::build_sprites(recipes, images, mappings).await
        };
        let (_, sprites) = tokio::join!(meta_fut, sprites_fut);

        for (id, res, warning) in sprites {
            if let Some((subject, message)) = warning {
                rm.diagnostics().warn(subject, || message);
            }
            let res = self.finish(rm.diagnostics(), &id, res);
            self.prime_raw(&id, res);
        }
    }

    pub async fn prime_meta(&self, rm: &ResourceManager, wanted: FastHashSet<ResourceId>) {
        if wanted.is_empty() {
            return;
        }

        let meta_ids: Vec<ResourceId> = wanted.into_iter().collect();
        let paths: Vec<String> = meta_ids.iter().map(ResourceId::texture_mcmeta_path).collect();
        let bytes = rm.read_many(&paths).await;

        let parsed = rayon_batch(meta_ids.into_iter().zip(bytes).collect(), |(id, bytes)| {
            let (meta, warning) = Self::parse_meta(bytes);
            (id, meta, warning)
        })
        .await;

        for (id, meta, warning) in parsed {
            if let Some(message) = warning {
                rm.diagnostics().warn(&id, || message);
            }
            let cell = self.meta.entry(id).or_default().value().clone();
            let _ = cell.set(meta);
        }
    }

    fn decode_image(
        id: &ResourceId,
        bytes: Option<Cow<'static, [u8]>>,
    ) -> Result<Arc<RgbaImage>, TextureError> {
        let bytes = bytes.ok_or_else(|| TextureError::NotFound(id.clone()))?;
        let img =
            image::load_from_memory(&bytes).map_err(|e| TextureError::Decode(id.clone(), e.to_string()))?;
        Ok(Arc::new(img.into_rgba8()))
    }

    async fn decode_sources(
        rm: &ResourceManager,
        image_ids: Vec<ResourceId>,
        palette_ids: Vec<ResourceId>,
    ) -> (Batch<ResourceId, Arc<RgbaImage>>, Batch<ResourceId, Palette>) {
        let mut paths: Vec<String> = Vec::with_capacity(image_ids.len() + palette_ids.len());
        paths.extend(image_ids.iter().map(ResourceId::texture_path));
        paths.extend(palette_ids.iter().map(ResourceId::palette_path));

        let mut bytes = rm.read_many(&paths).await;
        let palette_bytes = bytes.split_off(image_ids.len());

        let (images, palettes) = tokio::join!(
            rayon_batch(image_ids.iter().cloned().zip(bytes).collect(), |(id, b)| {
                let r = Self::decode_image(&id, b);
                (id, r)
            }),
            rayon_batch(
                palette_ids.iter().cloned().zip(palette_bytes).collect(),
                |(id, b)| {
                    let r = Self::decode_image(&id, b).map(|img| Palette::from_image(&img));
                    (id, r)
                }
            ),
        );

        (
            Arc::new(images.into_iter().collect::<FastHashMap<_, _>>()),
            Arc::new(palettes.into_iter().collect::<FastHashMap<_, _>>()),
        )
    }

    async fn build_mappings(
        recipes: &[Plan],
        palettes: Batch<ResourceId, Palette>,
    ) -> Batch<(ResourceId, ResourceId), PaletteMapping, MappingError> {
        let pairs: Vec<(ResourceId, ResourceId)> = recipes
            .iter()
            .filter_map(|(_, k)| match k {
                Some(SpriteRecipe::Palettized { key, palette, .. }) => Some((key.clone(), palette.clone())),
                _ => None,
            })
            .collect::<FastHashSet<_>>()
            .into_iter()
            .collect();

        let mappings = rayon_batch(pairs, move |(k, p)| {
            let res = match (palettes.get(&k), palettes.get(&p)) {
                (Some(Ok(kp)), Some(Ok(tp))) => PaletteMapping::create(kp, tp).map_err(MappingError::Size),
                (Some(Err(e)), _) | (_, Some(Err(e))) => Err(MappingError::Load(e.clone())),
                (None, _) => Err(MappingError::Load(TextureError::NotFound(k.clone()))),
                (_, None) => Err(MappingError::Load(TextureError::NotFound(p.clone()))),
            };
            ((k, p), res)
        })
        .await
        .into_iter()
        .collect::<FastHashMap<_, _>>();

        Arc::new(mappings)
    }

    async fn build_sprite(
        &self,
        rm: &ResourceManager,
        id: &ResourceId,
        recipe: &SpriteRecipe,
    ) -> Result<Arc<RgbaImage>, TextureError> {
        match recipe {
            SpriteRecipe::File(src) => Self::load_direct(rm, src).await,
            SpriteRecipe::Region { base, x, y, width, height, div_x, div_y } => {
                let img = Self::load_direct(rm, base).await?;

                let (rx, ry, rw, rh) =
                    match util::image::region_rect(&img, base, (*x, *y, *width, *height), *div_x, *div_y) {
                        Ok(bounds) => bounds,
                        Err(err) => {
                            rm.diagnostics().warn(id, || err);
                            return Err(TextureError::InvalidRegion {
                                sprite: id.clone(),
                                base: base.clone(),
                            });
                        }
                    };

                Ok(Arc::new(util::image::crop(&img, rx, ry, rw, rh)))
            }
            SpriteRecipe::Palettized { base, key, palette } => {
                let (src, key_palette, target_palette) = tokio::try_join!(
                    Self::load_direct(rm, base),
                    Self::load_palette(rm, key),
                    Self::load_palette(rm, palette),
                )?;

                let mapping = PaletteMapping::create(&key_palette, &target_palette).map_err(|err| {
                    rm.diagnostics()
                        .warn(palette.to_string(), || "Failed to create palette mapping");
                    TextureError::Palette(id.clone(), err)
                })?;

                Ok(Arc::new(mapping.remap(&src)))
            }
        }
    }

    async fn build_sprites(
        recipes: Vec<Plan>,
        sources: Batch<ResourceId, Arc<RgbaImage>>,
        mappings: Batch<(ResourceId, ResourceId), PaletteMapping, MappingError>,
    ) -> Vec<(ResourceId, Result<Arc<RgbaImage>, TextureError>, QueuedWarning)> {
        rayon_batch(recipes, move |(sprite, recipe)| {
            let get = |id: &ResourceId| match sources.get(id) {
                Some(Ok(img)) => Ok(img.clone()),
                Some(Err(e)) => Err(e.clone()),
                None => Err(TextureError::NotFound(id.clone())),
            };

            let mut warning = None;
            let res = match recipe {
                None => get(&sprite),

                Some(SpriteRecipe::File(id)) => get(&id),

                Some(SpriteRecipe::Region { base, x, y, width, height, div_x, div_y }) => get(&base)
                    .and_then(|img| {
                        let (rx, ry, rw, rh) = match util::image::region_rect(
                            &img,
                            &base,
                            (x, y, width, height),
                            div_x,
                            div_y,
                        ) {
                            Ok(bounds) => bounds,
                            Err(err) => {
                                warning = Some((sprite.to_string(), err));
                                return Err(TextureError::InvalidRegion {
                                    sprite: sprite.clone(),
                                    base: base.clone(),
                                });
                            }
                        };

                        Ok(Arc::new(util::image::crop(&img, rx, ry, rw, rh)))
                    }),

                Some(SpriteRecipe::Palettized { base, key, palette }) => {
                    let want = (key, palette.clone());
                    get(&base).and_then(|src| match mappings.get(&want) {
                        Some(Ok(m)) => Ok(Arc::new(m.remap(&src))),
                        Some(Err(MappingError::Size(e))) => {
                            warning = Some((
                                palette.to_string(),
                                "Failed to create palette mapping".to_string(),
                            ));
                            Err(TextureError::Palette(sprite.clone(), e.clone()))
                        }
                        Some(Err(MappingError::Load(e))) => Err(e.clone()),
                        None => Err(TextureError::NotFound(sprite.clone())),
                    })
                }
            };

            (sprite, res, warning)
        })
        .await
    }

    async fn index(&self, rm: &ResourceManager) -> Arc<SpriteIndex> {
        self.index
            .get_or_init(|| async { Arc::new(SpriteIndex::build(rm).await) })
            .await
            .clone()
    }

    fn finish(
        &self,
        diag: &Diagnostics,
        id: &ResourceId,
        result: Loaded<Arc<RgbaImage>>,
    ) -> Loaded<Arc<RgbaImage>> {
        match result {
            Err(TextureError::NotFound(ResourceId { namespace, path }))
                if &namespace == "minecraft" && &path == "missingno" =>
            {
                Ok(missing_texture().clone())
            }
            Err(err @ (TextureError::NotFound(_) | TextureError::InvalidRegion { .. }))
                if self.substitute_missing =>
            {
                diag.error(id, || format!("{err}. Substituting"));
                Ok(missing_texture().clone())
            }
            res => res,
        }
    }

    fn prime_raw(&self, id: &ResourceId, img: Loaded<Arc<RgbaImage>>) {
        let cell = self.raw.entry(id.clone()).or_default().value().clone();
        let _ = cell.set(img);
    }

    pub async fn get(&self, rm: &ResourceManager, id: &ResourceId) -> Loaded<Arc<RgbaImage>> {
        // clone the cell out so the shard lock is released before awaiting
        let cell = match self.raw.get(id) {
            Some(c) => c.value().clone(),
            None => self.raw.entry(id.clone()).or_default().value().clone(),
        };
        cell.get_or_init(|| self.resolve(rm, id)).await.clone()
    }
    
    pub async fn meta(&self, rm: &ResourceManager, id: &ResourceId) -> Arc<RawTextureMeta> {
        let index = self.index(rm).await;
        match index.recipe(id) {
            Some(SpriteRecipe::File(src)) => self.file_meta(rm, src).await,
            Some(_) => RawTextureMeta::empty(),
            None => self.file_meta(rm, id).await,
        }
    }

    async fn file_meta(&self, rm: &ResourceManager, id: &ResourceId) -> Arc<RawTextureMeta> {
        let cell = match self.meta.get(id) {
            Some(c) => c.value().clone(),
            None => self.meta.entry(id.clone()).or_default().value().clone(),
        };
        cell.get_or_init(|| async {
            let (meta, warning) = Self::parse_meta(rm.get_texture_mcmeta_bytes(id).await);
            if let Some(message) = warning {
                rm.diagnostics().warn(id, || message);
            }
            meta
        })
        .await
        .clone()
    }

    fn parse_meta(bytes: Option<Cow<'static, [u8]>>) -> (Arc<RawTextureMeta>, Option<String>) {
        let mut bytes = match bytes {
            Some(bytes) => bytes.into_owned(),
            None => return (RawTextureMeta::empty(), None),
        };
        match simd_json::serde::from_slice(&mut bytes) {
            Ok(meta) => (Arc::new(meta), None),
            Err(err) => (RawTextureMeta::empty(), Some(format!("Invalid .mcmeta: {err}"))),
        }
    }

    async fn resolve(&self, rm: &ResourceManager, id: &ResourceId) -> Loaded<Arc<RgbaImage>> {
        let index = self.index(rm).await;
        let result = match index.recipe(id) {
            Some(recipe) => self.build_sprite(rm, id, recipe).await,
            None => Self::load_direct(rm, id).await,
        };

        self.finish(rm.diagnostics(), id, result)
    }

    async fn load_direct(rm: &ResourceManager, id: &ResourceId) -> Loaded<Arc<RgbaImage>> {
        let bytes = rm.get_texture_bytes(id).await;

        Self::decode_image(id, bytes)
    }

    pub async fn rendered(
        &self,
        rm: &ResourceManager,
        id: &ResourceId,
        rotation: Quadrant,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Loaded<Arc<RgbaImage>> {
        let cell = self
            .rendered
            .entry(TextureOptions::new(id.clone(), rotation, from, to))
            .or_default()
            .value()
            .clone();

        cell.get_or_init(|| async {
            let source = self.get(rm, id).await?;

            let w = NonZeroU32::new(source.width()).ok_or_else(|| TextureError::Empty(id.clone()))?;
            let h = NonZeroU32::new(source.height()).ok_or_else(|| TextureError::Empty(id.clone()))?;

            // frame 0 or entire image if not animated
            let (fx, fy, fw, fh) = self
                .meta(rm, id)
                .await
                .frame_rect(w, h)
                .ok_or_else(|| TextureError::InvalidFrameSize(id.clone()))?;

            let out = match util::image::transform(&source, (fx, fy, fw, fh), from, to, rotation) {
                None => source,
                Some(img) => Arc::new(img),
            };
            Ok(out)
        })
        .await
        .clone()
    }

    async fn load_palette(rm: &ResourceManager, id: &ResourceId) -> Result<Palette, TextureError> {
        let bytes = rm
            .get_palette_bytes(id)
            .await
            .ok_or_else(|| TextureError::NotFound(id.clone()))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|e| TextureError::Decode(id.clone(), e.to_string()))?
            .into_rgba8();
        Ok(Palette::from_image(&image))
    }

    pub async fn sprite(
        &self,
        rm: &ResourceManager,
        id: &ResourceId,
        rotation: Quadrant,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Loaded<Arc<Sprite>> {
        let cell = self
            .sprites
            .entry(TextureOptions::new(id.clone(), rotation, from, to))
            .or_default()
            .value()
            .clone();

        cell.get_or_init(|| async {
            let image = self.rendered(rm, id, rotation, from, to).await?;
            Ok(Arc::new(Sprite::from_image(image)))
        })
        .await
        .clone()
    }
}

pub fn missing_texture() -> &'static Arc<RgbaImage> {
    static MISSING: OnceLock<Arc<RgbaImage>> = OnceLock::new();
    MISSING.get_or_init(|| {
        let mut img = RgbaImage::new(16, 16);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = if (y < 8) ^ (x < 8) {
                Rgba([0xF8, 0x00, 0xF8, 0xFF])
            } else {
                Rgba([0x00, 0x00, 0x00, 0xFF])
            };
        }
        Arc::new(img)
    })
}
