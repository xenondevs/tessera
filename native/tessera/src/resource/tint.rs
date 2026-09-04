use super::ResourceId;
use super::item::raw::RawTintSource;
use super::resource_manager::ResourceManager;
use crate::diagnostics::Diagnostics;
use std::borrow::Cow;

pub const COLOR_MAP_SIZE: usize = 256;

pub enum TintSource {
    Constant(u32),
    Dye { default: u32 },
    Grass { temperature: f32, downfall: f32 },
    Firework { default: u32 },
    Potion { default: u32 },
    MapColor { default: u32 },
    Team { default: u32 },
    RedstonePower,
    StemAge,
    None,
}

impl TintSource {
    pub fn color_item(&self, map: &ColorMap) -> Option<u32> {
        match self {
            TintSource::Constant(color) => Some(*color),
            TintSource::Dye { default }
            | TintSource::Firework { default }
            | TintSource::Potion { default }
            | TintSource::MapColor { default }
            | TintSource::Team { default } => Some(*default),
            TintSource::Grass { temperature, downfall } => Some(map.sample(*temperature, *downfall)),
            TintSource::RedstonePower | TintSource::StemAge => None, // block states
            TintSource::None => None,
        }
    }
}

#[rustfmt::skip]
impl From<RawTintSource> for TintSource {
    fn from(value: RawTintSource) -> Self {
        match value {
            RawTintSource::Constant { value } => TintSource::Constant(value.into_packed_color()),
            RawTintSource::Dye { default } => TintSource::Dye { default: default.into_packed_color() },
            RawTintSource::Grass { temperature, downfall } => TintSource::Grass { temperature, downfall },
            RawTintSource::Firework { default } => TintSource::Firework { default: default.into_packed_color() },
            RawTintSource::Potion { default } => TintSource::Potion { default: default.into_packed_color() },
            RawTintSource::MapColor { default } => TintSource::MapColor { default: default.into_packed_color() },
            RawTintSource::Team { default } => TintSource::Team { default: default.into_packed_color() },
            RawTintSource::Unknown => TintSource::None,
        }
    }
}

pub struct ColorMaps {
    pub grass: ColorMap,
    pub foliage: ColorMap,
    pub dry_foliage: ColorMap,
}

impl ColorMaps {
    const MAPS: [(&'static str, u32); 3] = [
        ("colormap/grass", 0xFFFF00FF), // https://mcsrc.dev/1/26.2/net/minecraft/world/level/GrassColor#L11
        ("colormap/foliage", 0xFF48B518), // https://mcsrc.dev/1/26.2/net/minecraft/world/level/FoliageColor#L15
        ("colormap/dry_foliage", 0xFF5C3C32), // https://mcsrc.dev/1/26.2/net/minecraft/world/level/DryFoliageColor#L12
    ];

    pub async fn load(rm: &ResourceManager, diag: &Diagnostics) -> Self {
        fn fill(
            pixels: &mut [u32; COLOR_MAP_SIZE * COLOR_MAP_SIZE],
            bytes: Option<Cow<'static, [u8]>>,
        ) -> Result<(), String> {
            let bytes = bytes.ok_or("No known pack provided the color map")?;
            let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.into_rgba8();

            if image.dimensions() != (COLOR_MAP_SIZE as u32, COLOR_MAP_SIZE as u32) {
                return Err(format!(
                    "Expected a {COLOR_MAP_SIZE}x{COLOR_MAP_SIZE} color map but it was {}x{}",
                    image.width(),
                    image.height()
                ));
            }
            for (slot, pixel) in pixels.iter_mut().zip(image.pixels()) {
                let [r, g, b, a] = pixel.0;
                *slot = u32::from_be_bytes([a, r, g, b]);
            }
            Ok(())
        }

        let paths: Vec<_> = Self::MAPS
            .iter()
            .map(|(path, _)| ResourceId::new_const("minecraft", path).texture_path())
            .collect();
        let mut bytes = rm.read_many(&paths).await;

        let [grass, foliage, dry_foliage] = Self::MAPS.map(|(path, default)| {
            let mut pixels = Box::new([default; COLOR_MAP_SIZE * COLOR_MAP_SIZE]);
            if let Err(e) = fill(&mut pixels, bytes.remove(0)) {
                diag.error(path, || {
                    format!("Failed to load color map image: {e} (using default color)")
                });
            }
            ColorMap { pixels, default }
        });

        Self { grass, foliage, dry_foliage }
    }
}

pub struct ColorMap {
    pixels: Box<[u32; COLOR_MAP_SIZE * COLOR_MAP_SIZE]>,
    default: u32,
}

impl ColorMap {
    pub fn sample(&self, temperature: f32, downfall: f32) -> u32 {
        let (temperature, downfall) = (temperature as f64, downfall as f64);
        let downfall = downfall * temperature;
        let x = ((1.0 - temperature) * 255.0) as u32;
        let y = ((1.0 - downfall) * 255.0) as u32;
        let idx = (y << 8 | x) as usize;
        self.pixels.get(idx).copied().unwrap_or(self.default)
    }
}
