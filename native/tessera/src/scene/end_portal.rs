use crate::resource::texture::sprite::Sprite;
use super::rasterize::{Source, Target};
use std::sync::Arc;
use ultraviolet::{Mat2, Vec2, Vec3};

/// [ref](https://github.com/InventivetalentDev/minecraft-assets/blob/26.3/assets/minecraft/shaders/core/rendertype_end_portal.fsh)
const LAYER_TINTS: [[f32; 3]; 16] = [
    [0.022087, 0.098399, 0.110818],
    [0.011892, 0.095924, 0.089485],
    [0.027636, 0.101689, 0.100326],
    [0.046564, 0.109883, 0.114838],
    [0.064901, 0.117696, 0.097189],
    [0.063761, 0.086895, 0.123646],
    [0.084817, 0.111994, 0.166380],
    [0.097489, 0.154120, 0.091064],
    [0.106152, 0.131144, 0.195191],
    [0.097721, 0.110188, 0.187229],
    [0.133516, 0.138278, 0.148582],
    [0.070006, 0.243332, 0.235792],
    [0.196766, 0.142899, 0.214696],
    [0.047281, 0.315338, 0.321970],
    [0.204675, 0.390010, 0.302066],
    [0.080955, 0.314821, 0.661491],
];

#[derive(Debug)]
pub struct EndPortal {
    sky: Arc<Sprite>,
    portal: Arc<Sprite>,
    /// Per layer, `uv = lin * (s, t) + off`
    layers: Vec<(Mat2, Vec2)>,
}

impl EndPortal {
    // TODO, allowing sampling game_time scroll in shade
    pub fn new(sky: Arc<Sprite>, portal: Arc<Sprite>, layers: u8, game_time: f32) -> Self {
        let layers = (1..=layers)
            .map(|l| {
                let l = l as f32;
                let theta = ((l * l * 4321.0 + l * 9.0) * 2.0).to_radians();
                let k = (4.5 - l / 4.0) * 2.0;
                let scroll = (2.0 + l / 1.5) * game_time * 1.5;
                let (sin, cos) = theta.sin_cos();
                // counterclockwise, columns
                let rot = Mat2::new(Vec2::new(cos, sin), Vec2::new(-sin, cos));
                (rot * (0.5 * k), Vec2::new(8.5 / l + 0.25, 0.5 * scroll + 0.25))
            })
            .collect();
        Self { sky, portal, layers }
    }

    fn shade(&self, s: f32, t: f32) -> u32 {
        let sample = |sprite: &Sprite, uv: Vec2| -> Vec3 {
            let x = ((uv.x - uv.x.floor()) * sprite.width as f32) as u32;
            let y = ((uv.y - uv.y.floor()) * sprite.height as f32) as u32;
            let [r, g, b, _] = sprite.image.get_pixel(x.min(sprite.width - 1), y.min(sprite.height - 1)).0;
            Vec3::new(r as f32, g as f32, b as f32) / 255.0
        };

        let mut color = sample(&self.sky, Vec2::new(s, t)) * Vec3::from(LAYER_TINTS[0]);
        for (i, (linear, offset)) in self.layers.iter().enumerate() {
            let uv = *linear * Vec2::new(s, t) + *offset;
            color += sample(&self.portal, uv) * Vec3::from(LAYER_TINTS[i]);
        }
        let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let Vec3 { x: r, y: g, z: b } = color;

        u32::from_le_bytes([channel(r), channel(g), channel(b), 255])
    }

    pub(super) fn as_source(&self, out: &Target) -> EndPortalSource<'_> {
        EndPortalSource {
            portal: self,
            screen: out.width.max(out.height).next_power_of_two().max(512) as f32,
        }
    }
}

pub(super) struct EndPortalSource<'a> {
    portal: &'a EndPortal,
    screen: f32,
}

impl Source for EndPortalSource<'_> {
    type Cursor = (i32, f32);

    #[inline(always)]
    fn cursor(&self, x: i32, y: i32, _: f32, _: f32) -> Self::Cursor {
        (x, 1.0 - (y as f32 + 0.5) / self.screen)
    }

    #[inline(always)]
    fn step(&self, cursor: &mut Self::Cursor) {
        cursor.0 += 1
    }

    #[inline(always)]
    fn color(&self, &(x, t): &Self::Cursor) -> u32 {
        self.portal.shade((x as f32 + 0.5) / self.screen, t)
    }
}
