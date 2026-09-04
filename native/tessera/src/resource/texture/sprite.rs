use super::cache::missing_texture;
use image::RgbaImage;
use std::sync::{Arc, OnceLock};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Blend {
    // A=1
    Opaque,
    // A=0 or A=1
    Cutout,
    // A in 0..1
    Translucent,
}

impl Blend {
    pub fn scan(img: &RgbaImage) -> Blend {
        let mut clear = false;
        let raw = img.as_raw().as_chunks::<4>().0;
        for texel in raw {
            match texel[3] {
                0 => clear = true,
                255 => continue,
                _ => return Blend::Translucent,
            }
        }
        if clear { Blend::Cutout } else { Blend::Opaque }
    }
}

#[derive(Clone, Debug)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub image: Arc<RgbaImage>,
    pub blend: Blend,
}

impl Sprite {
    pub fn from_image(image: Arc<RgbaImage>) -> Self {
        let (width, height) = image.dimensions();
        let blend = Blend::scan(&image);
        Self { width, height, image, blend }
    }

    #[inline(always)]
    pub fn texel(&self, x: u32, y: u32) -> u32 {
        let pixels = self.image.as_chunks::<4>().0;
        u32::from_le_bytes(pixels[(y * self.width + x) as usize])
    }
}

pub fn missing_sprite() -> &'static Arc<Sprite> {
    static MISSING: OnceLock<Arc<Sprite>> = OnceLock::new();
    MISSING.get_or_init(|| Arc::new(Sprite::from_image(missing_texture().clone())))
}
