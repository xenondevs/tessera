use image::RgbaImage;
use png::{BitDepth, ColorType, Compression, Encoder, Filter};

const SCAN_WORD: usize = 16;
const ALPHAS: u128 = 0xFF00_0000_FF00_0000_FF00_0000_FF00_0000;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min_x: u32,
    pub max_x: u32,
    pub min_y: u32,
    pub max_y: u32,
}

pub fn encode(image: &RgbaImage) -> (Vec<u8>, Option<Bounds>) {
    match palletize(image) {
        Some((palette, indices)) => {
            let bounds = find_bounds(image, Some((&palette, &indices)));
            let png = encode_indexed(image.width(), image.height(), palette, indices);
            (png, bounds)
        }
        None => (encode_rgba(image), find_bounds(image, None)),
    }
}

fn palletize(image: &RgbaImage) -> Option<(Vec<[u8; 4]>, Vec<u8>)> {
    const SLOTS: usize = 4096;
    const EMPTY: u16 = u16::MAX;

    let mut slots = [0u32; SLOTS];
    let mut slot_index = [EMPTY; SLOTS];
    let mut palette: Vec<[u8; 4]> = Vec::with_capacity(64);
    let pixels = image.as_raw().as_chunks::<4>().0;
    let mut indices = Vec::with_capacity(pixels.len());
    for pixel in pixels {
        let color = u32::from_le_bytes(*pixel);
        // https://probablydance.com/2018/06/16/fibonacci-hashing-the-optimization-that-the-world-forgot-or-a-better-alternative-to-integer-modulo/
        let slot = (color.wrapping_mul(0x9E37_79B1) >> 20) as usize & (SLOTS - 1);
        let idx = if slot_index[slot] != EMPTY && slots[slot] == color {
            slot_index[slot] as u8
        } else {
            let idx = match palette.iter().position(|entry| entry == pixel) {
                Some(idx) => idx as u8,
                None => {
                    if palette.len() == 256 {
                        return None;
                    }
                    palette.push(*pixel);
                    (palette.len() - 1) as u8
                }
            };
            slots[slot] = color;
            slot_index[slot] = u16::from(idx);
            idx
        };
        indices.push(idx);
    }

    Some((palette, indices))
}

fn encode_indexed(width: u32, height: u32, mut palette: Vec<[u8; 4]>, mut indices: Vec<u8>) -> Vec<u8> {
    // sort so non-opaque colors come first
    let translucent = palette.iter().filter(|color| color[3] != 255).count();
    if !palette[..translucent].iter().all(|color| color[3] != 255) {
        let mut remap = [0u8; 256];
        let mut sorted = Vec::with_capacity(palette.len());
        for tr_pass in [true, false] {
            for (idx, color) in palette.iter().enumerate() {
                if (color[3] != 255) == tr_pass {
                    remap[idx] = sorted.len() as u8;
                    sorted.push(*color);
                }
            }
        }
        palette = sorted;
        indices.iter_mut().for_each(|idx| *idx = remap[usize::from(*idx)])
    }

    let mut out = Vec::with_capacity(indices.len() / 2);
    let mut encoder = Encoder::new(&mut out, width, height);
    encoder.set_color(ColorType::Indexed);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_palette(
        palette
            .iter()
            .flat_map(|color| [color[0], color[1], color[2]])
            .collect::<Vec<u8>>(),
    );
    if translucent > 0 {
        encoder.set_trns(palette[..translucent].iter().map(|color| color[3]).collect::<Vec<u8>>());
    }
    encoder.set_compression(Compression::Fast);
    encoder.set_filter(Filter::Adaptive);

    let mut writer = encoder.write_header().expect("the header matches the data");
    writer.write_image_data(&indices).expect("writing to a vec cant fail");
    writer.finish().expect("writing to a vec cant fail");
    out
}

fn encode_rgba(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(image.as_raw().len() / 4);
    let mut encoder = Encoder::new(&mut out, image.width(), image.height());
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_compression(Compression::Fast);
    let mut writer = encoder.write_header().expect("the header matches the data");
    writer
        .write_image_data(image.as_raw())
        .expect("writing to a vec cant fail");
    writer.finish().expect("writing to a vec cant fail");
    out
}

pub(crate) fn find_bounds(image: &RgbaImage, palettized: Option<(&[[u8; 4]], &[u8])>) -> Option<Bounds> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    if width == 0 || height == 0 {
        return None;
    }
    if let Some((palette, indices)) = palettized {
        let mut transparent = palette
            .iter()
            .enumerate()
            .filter(|(_, color)| color[3] == 0)
            .map(|(i, _)| i);
        match (transparent.next(), transparent.next()) {
            (None, _) => {
                return Some(Bounds {
                    min_x: 0,
                    max_x: width as u32 - 1,
                    min_y: 0,
                    max_y: height as u32 - 1,
                });
            }
            (Some(idx), None) if width >= SCAN_WORD => {
                let transparent = u128::from_le_bytes([idx as u8; SCAN_WORD]);
                return scan(indices, width, 1, |word| word ^ transparent);
            }
            _ => {}
        }
    }

    if width * 4 >= SCAN_WORD {
        return scan(image.as_raw(), width * 4, 4, |word| word & ALPHAS);
    }

    let (mut left, mut right, mut top, mut bottom) = (usize::MAX, 0, usize::MAX, 0);
    for (y, row) in image.as_raw().chunks_exact(width * 4).enumerate() {
        for (x, pixel) in row.as_chunks::<4>().0.iter().enumerate() {
            if pixel[3] != 0 {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = y;
            }
        }
    }

    if top == usize::MAX {
        None
    } else {
        Some(Bounds {
            min_x: left as u32,
            max_x: right as u32,
            min_y: top as u32,
            max_y: bottom as u32,
        })
    }
}

fn scan<V: Fn(u128) -> u128>(bytes: &[u8], stride: usize, unit: usize, visible: V) -> Option<Bounds> {
    let height = bytes.len() / stride;
    let load = |at: usize| {
        visible(u128::from_le_bytes(
            bytes[at..at + SCAN_WORD].try_into().expect("a word of bytes"),
        ))
    };

    let row_or = |y: usize| {
        let (base, mut acc, mut at) = (y * stride, 0, 0);
        while at + SCAN_WORD <= stride {
            acc |= load(base + at);
            at += SCAN_WORD;
        }
        acc | load(base + stride - SCAN_WORD)
    };
    let top = (0..height).find(|y| row_or(*y) != 0)?;
    let bottom = (top..height).rfind(|y| row_or(*y) != 0).expect("the top row has a pixel");

    // pixel a lowest/highest bit belongs to
    let first = |word: u128, at: usize| (at * 8 + word.trailing_zeros() as usize) / (8 * unit);
    let last = |word: u128, at: usize| (at * 8 + 127 - word.leading_zeros() as usize) / (8 * unit);

    let (mut left, mut end) = (stride / unit, 0);
    for seed in 0..=4 {
        let base = (top + (bottom - top) * seed / 4) * stride;
        // from left up to the guess then from right down to the guess.
        let mut at = 0;
        while at < left * unit {
            let offset = at.min(stride - SCAN_WORD);
            let word = load(base + offset);
            if word != 0 {
                left = left.min(first(word, offset));
                break;
            }
            at += SCAN_WORD;
        }
        let mut at = stride;
        while at > end * unit {
            let offset = at.saturating_sub(SCAN_WORD);
            let word = load(base + offset);
            if word != 0 {
                end = end.max(last(word, offset) + 1);
                break;
            }
            at = offset;
        }
    }

    let column = |offset: usize, mask: u128| (top..=bottom).fold(0, |acc, y| acc | load(y * stride + offset)) & mask;
    let span = |from: usize, to: usize| match to - from {
        SCAN_WORD => u128::MAX,
        count => ((1u128 << (8 * count)) - 1) << (8 * from),
    };

    let (mut at, stop) = (0, left * unit);
    while at < stop {
        let offset = at.min(stride - SCAN_WORD);
        let to = (stop - offset).min(SCAN_WORD);
        let hit = column(offset, span(at - offset, to));
        if hit != 0 {
            left = first(hit, offset);
            break;
        }
        at = offset + to;
    }
    let (mut at, stop) = (stride, end * unit);
    while at > stop {
        let offset = at.saturating_sub(SCAN_WORD);
        let from = stop.max(offset) - offset;
        let hit = column(offset, span(from, at - offset));
        if hit != 0 {
            end = last(hit, offset) + 1;
            break;
        }
        at = offset + from;
    }

    Some(Bounds {
        min_x: left as u32,
        max_x: end as u32 - 1,
        min_y: top as u32,
        max_y: bottom as u32,
    })
}
