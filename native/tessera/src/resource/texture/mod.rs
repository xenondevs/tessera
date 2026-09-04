pub mod atlas;
pub mod cache;
pub mod face;
pub mod mcmeta;
pub mod palette;
pub mod sprite;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawSlot {
    Simple(String),
    Full {
        sprite: String,
        #[serde(default)]
        force_translucent: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "RawSlot")]
pub struct TextureSlot {
    /// The id of the sprite or a `#reference` until the texture is resolved.
    pub sprite: String,
    pub force_translucent: bool,
}

impl TextureSlot {
    pub fn reference_target(&self) -> Option<&str> {
        self.sprite.strip_prefix('#')
    }
}

impl From<RawSlot> for TextureSlot {
    fn from(raw: RawSlot) -> Self {
        match raw {
            RawSlot::Simple(sprite) => Self { sprite, force_translucent: false },
            RawSlot::Full { sprite, force_translucent } => Self { sprite, force_translucent },
        }
    }
}
