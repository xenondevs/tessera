use image::{Rgba, RgbaImage};
use thiserror::Error;

const EMPTY_PACKED: u32 = u32::MAX;
/// floor(2^32 / φ), where φ is the golden ratio. [Ref](https://probablydance.com/2018/06/16/fibonacci-hashing-the-optimization-that-the-world-forgot-or-a-better-alternative-to-integer-modulo/)
const FIB_HASH_MULTIPLIER: u32 = 0x9E37_79B9;

/// Every pixel of a palette image in row-major order, dupes allowed. Same as [RgbaImage::pixels].
#[derive(Clone, Debug, Default)]
pub struct Palette {
    colors: Vec<Rgba<u8>>,
}

impl Palette {
    pub fn from_image(image: &RgbaImage) -> Self {
        Self { colors: image.pixels().copied().collect() }
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("Palette size mismatch: base has {base} entries, target has {target}")]
pub struct PaletteSizeMismatch {
    pub base: usize,
    pub target: usize,
}

/// One-to-one mapping from a key palette to a target palette, keyed by Rgb. Key alpha channel is ignored.
#[derive(Clone, Debug)]
pub struct PaletteMapping {
    slots: Box<[(u32, Rgba<u8>)]>,
    mask: usize,
    shift: u32,
    len: usize,
}

impl PaletteMapping {
    #[inline(always)]
    fn slot(&self, key: u32) -> usize {
        (key.wrapping_mul(FIB_HASH_MULTIPLIER) >> self.shift) as usize
    }

    /// Fully transparent pixels are skipped allowing the key image to mask off pixels
    /// it doesnt want to control.
    pub fn create(key: &Palette, target: &Palette) -> Result<Self, PaletteSizeMismatch> {
        if key.len() != target.len() {
            return Err(PaletteSizeMismatch { base: key.len(), target: target.len() });
        }

        let cap = (key.len() * 2).next_power_of_two().max(8);
        let mask = cap - 1;
        let shift = 32 - cap.trailing_zeros();

        let mut slots = vec![(EMPTY_PACKED, Rgba([0, 0, 0, 0])); cap].into_boxed_slice();
        let mut len = 0;

        for (k, t) in key.colors.iter().zip(target.colors.iter()) {
            // transparent
            if k[3] == 0 {
                continue;
            }

            let packed = pack(*k);
            let mut i = (packed.wrapping_mul(FIB_HASH_MULTIPLIER) >> shift) as usize;
            loop {
                match slots[i].0 {
                    EMPTY_PACKED => {
                        slots[i] = (packed, *t);
                        len += 1;
                        break;
                    }

                    existing if existing == packed => {
                        slots[i].1 = *t;
                        break;
                    }
                    _ => i = (i + 1) & mask,
                }
            }
        }

        Ok(Self { slots, mask, shift, len })
    }

    /// * Fully transparent source pixels are returned untouched
    /// * Colors that arent in the table are returned untouched
    /// * For any mapped color the alphas multiply (`srcA * targetA / 255`). So a semi-transparent
    ///   source stays semi-transparent.
    pub fn apply(&self, source: Rgba<u8>) -> Rgba<u8> {
        if source[3] == 0 {
            return source;
        }

        let packed = pack(source);
        let mut i = self.slot(packed);
        loop {
            let (k, v) = self.slots[i];
            if k == packed {
                let alpha = (source[3] as u16 * v[3] as u16 / 255) as u8;
                return Rgba([v[0], v[1], v[2], alpha]);
            }
            if k == EMPTY_PACKED {
                return source;
            }
            i = (i + 1) & self.mask;
        }
    }

    pub fn remap(&self, source: &RgbaImage) -> RgbaImage {
        let data: Vec<u8> = source.pixels().flat_map(|p| self.apply(*p).0).collect();
        RgbaImage::from_raw(source.width(), source.height(), data).expect("4 bytes per source pixel")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[inline(always)]
fn pack(color: Rgba<u8>) -> u32 {
    (color[0] as u32) << 16 | (color[1] as u32) << 8 | (color[2] as u32)
}
