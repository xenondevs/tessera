use serde::Deserialize;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};

#[derive(Debug, Default, Deserialize)]
pub struct RawTextureMeta {
    pub animation: Option<AnimationSection>,
    pub texture: Option<TextureSection>,
}

#[derive(Debug, Deserialize)]
pub struct AnimationSection {
    pub frames: Option<Vec<AnimationFrame>>,
    pub width: Option<NonZeroU32>,
    pub height: Option<NonZeroU32>,
    #[serde(default = "default_frametime")]
    pub frametime: NonZeroU32,
    #[serde(default)]
    pub interpolate: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AnimationFrame {
    Index(u32),
    Full { index: u32, time: Option<u32> },
}

impl AnimationFrame {
    pub fn index(&self) -> u32 {
        match self {
            AnimationFrame::Index(index) => *index,
            AnimationFrame::Full { index, .. } => *index,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct TextureSection {
    #[serde(default)]
    pub blur: bool,
    #[serde(default)]
    pub clamp: bool,
    #[serde(default)]
    pub mipmap_strategy: MipmapStrategy,
    #[serde(default)]
    pub alpha_cutoff_bias: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MipmapStrategy {
    #[default]
    Auto,
    Mean,
    Cutout,
    StrictCutout,
    DarkCutout,
}

impl RawTextureMeta {
    #[inline(always)]
    pub fn empty() -> Arc<Self> {
        static EMPTY: LazyLock<Arc<RawTextureMeta>> = LazyLock::new(|| Arc::new(RawTextureMeta::default()));

        EMPTY.clone()
    }

    /// Rect of frame 0 in the source sprite `(x, y, w, h)` or, if there is no animation, the
    /// whole image. `None` when the frame size does not divide the image evenly.
    pub fn frame_rect(&self, w: NonZeroU32, h: NonZeroU32) -> Option<(u32, u32, u32, u32)> {
        let Some(anim) = &self.animation else {
            return Some((0, 0, w.get(), h.get()));
        };
        let (fw, fh) = match (anim.width, anim.height) {
            (Some(fw), Some(fh)) => (fw.get(), fh.get()),
            (Some(fw), None) => (fw.get(), h.get()),
            (None, Some(fh)) => (w.get(), fh.get()),
            (None, None) => {
                let square = w.min(h).get();
                (square, square)
            }
        };
        if !w.get().is_multiple_of(fw) || !h.get().is_multiple_of(fh) {
            return None;
        }
        let first = anim
            .frames
            .as_ref()
            .and_then(|f| f.first())
            .map_or(0, AnimationFrame::index);
        let per_row = w.get() / fw;
        Some((first % per_row * fw, first / per_row * fh, fw, fh))
    }
}

fn default_frametime() -> NonZeroU32 {
    NonZeroU32::MIN
}
