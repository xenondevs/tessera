use crate::diagnostics::Diagnostics;
use crate::resource::ResourceId;
use crate::resource::resource_manager::ResourceManager;
use crate::util::{CONCURRENCY, FastHashMap};
use futures_util::{StreamExt, stream};
use regex::Regex;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use tessera_derive::mc_registry_enum;

/// [Vanilla ref](https://mcsrc.dev/1/26.2/net/minecraft/client/resources/model/sprite/AtlasManager#L33)
///
/// Order matters because these all share one sprite namespace, so a later atlas overwrites an earlier
/// one on collision. `items` after `blocks` (i.e. `items` atlas having a higher prio) matches
/// vanilla's `CombinedBlockItemMaterialBaker`.
const KNOWN_ATLASES: [&str; 12] = [
    "banner_patterns", "blocks", "celestials", "chests", "decorated_pot", "gui", "items", "map_decorations",
    "paintings", "particles", "shield_patterns", "shulker_boxes",
];

#[derive(Debug, Deserialize)]
pub struct RawAtlas {
    pub sources: Vec<RawSpriteSource>,
}

#[mc_registry_enum]
#[derive(Debug, Deserialize)]
pub enum RawSpriteSource {
    PalettedPermutations {
        textures: Vec<ResourceId>,
        palette_key: ResourceId,
        /// Keys are permutation names (`quartz`, `iron`) appended to each texture id via
        /// `separator`.
        permutations: HashMap<String, ResourceId>,
        #[serde(default = "default_separator")]
        separator: Cow<'static, str>,
    },
    Single {
        resource: ResourceId,
        sprite: Option<ResourceId>,
    },
    Directory {
        source: String,
        prefix: String,
    },
    Filter {
        pattern: RawIdentifierPattern,
    },
    Unstitch {
        resource: ResourceId,
        regions: Vec<UnstitchRegion>,
        #[serde(default = "default_divisor", rename = "divisor_x")]
        div_x: f64,
        #[serde(default = "default_divisor", rename = "divisor_y")]
        div_y: f64,
    },
}

#[derive(Debug, Deserialize)]
pub struct RawIdentifierPattern {
    pub namespace: Option<String>,
    pub path: Option<String>,
}

impl RawIdentifierPattern {
    /// Compile both halves of an [`RawIdentifierPattern`] into an [`IdentifierMatcher`]. A `None`
    /// pattern matches everything. A failed compilation leads to [`IdentifierMatcher::Disabled`]
    /// (i.e. matches nothing).
    pub fn compile(&self, subject: &str, diag: &Diagnostics) -> IdentifierMatcher {
        let compile_one = |source: &Option<String>, field: &str| match source {
            None => Ok(None),
            Some(source) => match Regex::new(source) {
                Ok(regex) => Ok(Some(regex)),
                Err(e) => {
                    diag.warn_keyed(subject, field, || {
                        format!("invalid \"{field}\" filter pattern {source:?}: {e}")
                    });
                    Err(())
                }
            },
        };

        match (
            compile_one(&self.namespace, "namespace"),
            compile_one(&self.path, "path"),
        ) {
            (Ok(namespace), Ok(path)) => IdentifierMatcher::Active { namespace, path },
            _ => IdentifierMatcher::Disabled,
        }
    }
}

/// A compiled [`RawIdentifierPattern`]. An absent pattern matches everything, so `None` is
/// equivalent to `.*`. A disabled pattern matches nothing.
pub enum IdentifierMatcher {
    Active {
        namespace: Option<Regex>,
        path: Option<Regex>,
    },
    Disabled,
}

impl IdentifierMatcher {
    pub fn matches(&self, id: &ResourceId) -> bool {
        match self {
            Self::Disabled => false,
            Self::Active { namespace, path } => {
                namespace.as_ref().is_none_or(|re| re.is_match(&id.namespace))
                    && path.as_ref().is_none_or(|re| re.is_match(&id.path))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UnstitchRegion {
    pub sprite: ResourceId,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Default)]
pub struct SpriteIndex {
    pub sprites: FastHashMap<ResourceId, SpriteRecipe>,
}

impl SpriteIndex {
    pub async fn build(rm: &ResourceManager) -> Self {
        let atlas_loads = stream::iter(KNOWN_ATLASES.iter().cloned())
            .map(|atlas| async move {
                let stack = rm.get_atlas_stack(atlas).await;
                (atlas, stack)
            })
            .buffered(CONCURRENCY.min(KNOWN_ATLASES.len()));

        let (stacks, all_textures) = tokio::join!(atlas_loads.collect::<Vec<_>>(), rm.list(None, ".png"));

        let parsed: Vec<(&str, &str)> = all_textures
            .iter()
            .filter_map(|p| split_texture_path(p.as_ref()))
            .collect();

        let mut index = Self::default();
        for (atlas, stack) in stacks {
            let subject = format!("atlases/{atlas}.json");

            let mut sprites = FastHashMap::default();
            for (layer, bytes) in stack.into_iter().enumerate() {
                let mut bytes = bytes.into_owned();
                match simd_json::serde::from_slice::<RawAtlas>(&mut bytes) {
                    Ok(atlas) => {
                        for source in &atlas.sources {
                            Self::run(&mut sprites, &subject, rm.diagnostics(), source, &parsed)
                        }
                    }
                    Err(err) => rm
                        .diagnostics()
                        .warn_keyed(&subject, layer, || format!("invalid atlas: {err}")),
                }
            }
            index.sprites.extend(sprites.into_iter());
        }

        index
    }

    pub fn recipe(&self, id: &ResourceId) -> Option<&SpriteRecipe> {
        self.sprites.get(id)
    }

    fn run(
        sprites: &mut FastHashMap<ResourceId, SpriteRecipe>,
        subject: &str,
        diagnostics: &Diagnostics,
        source: &RawSpriteSource,
        all_textures: &Vec<(&str, &str)>,
    ) {
        match source {
            RawSpriteSource::Single { resource, sprite } => {
                let id = sprite.clone().unwrap_or_else(|| resource.clone());
                sprites.insert(id, SpriteRecipe::File(resource.clone()));
            }
            RawSpriteSource::Filter { pattern } => {
                let matcher = pattern.compile(subject, diagnostics);
                sprites.retain(|id, _| !matcher.matches(id));
            }
            RawSpriteSource::Directory { source, prefix } => {
                let dir = format!("{source}/");
                for (namespace, path) in all_textures {
                    let Some(tail) = path.strip_prefix(&dir) else { continue };
                    sprites.insert(
                        ResourceId::new(namespace.to_string(), format!("{prefix}{tail}")),
                        SpriteRecipe::File(ResourceId::new(namespace.to_string(), path.to_string())),
                    );
                }
            }
            RawSpriteSource::Unstitch { resource, regions, div_x, div_y } => {
                for region in regions {
                    sprites.insert(
                        region.sprite.clone(),
                        SpriteRecipe::Region {
                            base: resource.clone(),
                            x: region.x,
                            y: region.y,
                            width: region.width,
                            height: region.height,
                            div_x: *div_x,
                            div_y: *div_y,
                        },
                    );
                }
            }
            RawSpriteSource::PalettedPermutations { textures, palette_key, permutations, separator } => {
                for base in textures {
                    for (name, palette) in permutations {
                        let id = ResourceId::new(
                            base.namespace.clone().into_owned(),
                            format!("{}{separator}{name}", base.path),
                        );
                        sprites.insert(
                            id,
                            SpriteRecipe::Palettized {
                                base: base.clone(),
                                key: palette_key.clone(),
                                palette: palette.clone(),
                            },
                        );
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub enum SpriteRecipe {
    /// plain texture file
    File(ResourceId),
    /// unstitch, region of a larger sheet of textures
    Region {
        base: ResourceId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        div_x: f64,
        div_y: f64,
    },
    /// paletted permutations
    Palettized {
        base: ResourceId,
        key: ResourceId,
        palette: ResourceId,
    },
}

pub fn default_separator() -> Cow<'static, str> {
    Cow::Borrowed("_")
}

pub fn default_divisor() -> f64 {
    1.0
}

fn split_texture_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("assets/")?;
    let (namespace, rest) = rest.split_once('/')?;
    let rest = rest.strip_prefix("textures/")?;
    let (rest, ext) = rest.split_at_checked(rest.len().checked_sub(".png".len())?)?;
    Some((namespace, ext.eq_ignore_ascii_case(".png").then_some(rest)?))
}
