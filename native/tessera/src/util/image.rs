use crate::direction::Quadrant;
use crate::resource::ResourceId;
use image::imageops::FilterType;
use image::{RgbaImage, imageops};
use std::ops::Range;

/// Crop rect in source pixel coords + flips implied by inverted uvs
struct Window {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    flip_x: bool,
    flip_y: bool,
}

fn window(src: &RgbaImage, frame: (u32, u32, u32, u32), from: (f32, f32), to: (f32, f32)) -> Window {
    let (fx, fy, fw, fh) = frame;
    let to_px = |v: f32, size: u32| (v * size as f32).round().clamp(0.0, size as f32) as u32;
    let (x0, y0) = (to_px(from.0, fw), to_px(from.1, fh));
    let (x1, y1) = (to_px(to.0, fw), to_px(to.1, fh));
    let (lo_x, lo_y) = (x0.min(x1), y0.min(y1));
    let (hi_x, hi_y) = (x0.max(x1), y0.max(y1));

    let ox = (fx + lo_x).min(src.width());
    let oy = (fy + lo_y).min(src.height());

    Window {
        x: ox,
        y: oy,
        width: (hi_x - lo_x).min(src.width() - ox),
        height: (hi_y - lo_y).min(src.height() - oy),
        flip_x: x0 > x1,
        flip_y: y0 > y1,
    }
}

pub fn transform(
    src: &RgbaImage,
    frame: (u32, u32, u32, u32),
    from: (f32, f32),
    to: (f32, f32),
    rotation: Quadrant,
) -> Option<RgbaImage> {
    let w = window(src, frame, from, to);
    if w.width == 0 || w.height == 0 {
        return Some(RgbaImage::new(1, 1));
    }

    let (crop_w, crop_h) = (w.width, w.height);
    let src_width = src.width() as usize;
    let src_buf = src.as_raw();

    match rotation {
        // row -> row copies
        Quadrant::R0 | Quadrant::R180 => {
            let half_turn = rotation == Quadrant::R180;
            let (flip_x, flip_y) = (w.flip_x ^ half_turn, w.flip_y ^ half_turn);

            if !flip_x && !flip_y && (w.x, w.y) == (0, 0) && (crop_w, crop_h) == src.dimensions() {
                return None;
            }

            let mut out = RgbaImage::new(crop_w, crop_h);
            let out_buf: &mut [u8] = &mut out;
            let row_bytes = crop_w as usize * 4;

            if !flip_x && !flip_y && w.x == 0 && crop_w as usize == src_width {
                let src_offset = w.y as usize * src_width * 4;
                out_buf.copy_from_slice(&src_buf[src_offset..src_offset + row_bytes * crop_h as usize]);
                return Some(out);
            }

            for out_y in 0..crop_h {
                let crop_y = if flip_y { crop_h - 1 - out_y } else { out_y };
                let src_offset = ((w.y + crop_y) as usize * src_width + w.x as usize) * 4;
                let src_row = &src_buf[src_offset..src_offset + row_bytes];
                let out_row = &mut out_buf[out_y as usize * row_bytes..][..row_bytes];

                if flip_x {
                    // lets llvm vectorize
                    for (o, i) in out_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4).rev()) {
                        o.copy_from_slice(i);
                    }
                } else {
                    out_row.copy_from_slice(src_row);
                }
            }
            Some(out)
        }
        // col -> row copies
        Quadrant::R90 | Quadrant::R270 => {
            #[rustfmt::skip]
            fn transpose_block(
                out_buf: &mut [u8], src_buf: &[u8], src_width: usize, w: &Window,
                (x_base, x_step): (isize, isize), (y_base, y_step): (isize, isize),
                rows: Range<usize>, cols: Range<usize>, stride: usize,
            ) {
                for out_y in rows {
                    let crop_x = (x_base + x_step * out_y as isize) as usize;
                    let column = (w.x as usize + crop_x) * 4;
                    let row_start = out_y * stride;
                    for out_x in cols.clone() {
                        let crop_y = (y_base + y_step * out_x as isize) as usize;
                        let src_offset = (w.y as usize + crop_y) * src_width * 4 + column;
                        let out_offset = row_start + out_x * 4;
                        out_buf[out_offset..out_offset + 4]
                            .copy_from_slice(&src_buf[src_offset..src_offset + 4]);
                    }
                }
            }

            let is_quarter = rotation == Quadrant::R90;
            let (x_base, x_step) = if is_quarter == w.flip_x {
                (crop_w as isize - 1, -1isize)
            } else {
                (0, 1)
            };

            let (y_base, y_step) = if is_quarter != w.flip_y {
                (crop_h as isize - 1, -1isize)
            } else {
                (0, 1)
            };

            let (out_width, out_height) = (crop_h as usize, crop_w as usize);
            let mut out = RgbaImage::new(crop_h, crop_w);
            let out_buf: &mut [u8] = &mut out;
            let stride = out_width * 4;
            let strides = ((x_base, x_step), (y_base, y_step));
            if src_buf.len() < 512 {
                #[rustfmt::skip]
                transpose_block(
                    out_buf, src_buf, src_width, &w,
                    strides.0, strides.1,
                    0..out_height, 0..out_width, stride
                );
            } else {
                const TILE: usize = 16;
                for ty in (0..out_height).step_by(TILE) {
                    for tx in (0..out_width).step_by(TILE) {
                        #[rustfmt::skip]
                        transpose_block(
                            out_buf, src_buf, src_width, &w,
                            strides.0, strides.1,
                            ty..(ty + TILE).min(out_height), tx..(tx + TILE).min(out_width), stride
                        );
                    }
                }
            }

            Some(out)
        }
    }
}

pub fn crop(src: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    let x = x.min(src.width());
    let y = y.min(src.height());
    let w = w.min(src.width() - x);
    let h = h.min(src.height() - y);

    let src_width = src.width() as usize;
    let src_buf = src.as_raw();
    let row_bytes = w as usize * 4;
    let mut out = RgbaImage::new(w, h);
    let out_buf: &mut [u8] = &mut out;
    for (row, out_row) in out_buf.chunks_exact_mut(row_bytes).enumerate() {
        let src_offset = ((y as usize + row) * src_width + x as usize) * 4;
        out_row.copy_from_slice(&src_buf[src_offset..src_offset + row_bytes]);
    }
    out
}

pub fn region_rect(
    img: &RgbaImage,
    base: &ResourceId,
    rect: (f64, f64, f64, f64),
    div_x: f64,
    div_y: f64,
) -> Result<(u32, u32, u32, u32), String> {
    let (x, y, w, h) = rect;
    let (sx, sy) = (img.width() as f64 / div_x, img.height() as f64 / div_y);
    let (rx, ry) = ((x * sx).floor(), (y * sy).floor());
    let (rw, rh) = ((w * sx).floor(), (h * sy).floor());

    if !(rw >= 1.0
        && rh >= 1.0
        && rx >= 0.0
        && ry >= 0.0
        && rx + rw <= img.width() as f64
        && ry + rh <= img.height() as f64)
    {
        return Err(format!(
            "Unstitch region ({rx}, {ry}) {rw}x{rh} does not fit in the {}x{} source {base}",
            img.width(),
            img.height()
        ));
    }
    Ok((rx as u32, ry as u32, rw as u32, rh as u32))
}

pub fn scale(src: &RgbaImage, width: u32, height: u32) -> Option<RgbaImage> {
    let (src_width, src_height) = src.dimensions();
    let replicate = || -> Option<RgbaImage> {
        if src_width == 0 || src_height == 0 || width % src_width != 0 || height % src_height != 0 {
            return None;
        }
        let (fx, fy) = ((width / src_width) as usize, (height / src_height) as usize);
        let stride = width as usize * 4;
        let src_buf = src.as_raw();
        let mut out = RgbaImage::new(width, height);
        let out_buf: &mut [u8] = &mut out;
        let mut row = vec![0u8; stride];

        for y in 0..src_height as usize {
            for x in 0..src_width as usize {
                let src_offset = (y * src_width as usize + x) * 4;
                let Some(texel): Option<[u8; 4]> = src_buf[src_offset..src_offset + 4].try_into().ok() else {
                    return None;
                };
                for cell in row[x * fx * 4..(x + 1) * fx * 4].chunks_exact_mut(4) {
                    cell.copy_from_slice(&texel);
                }
            }
            for step in 0..fy {
                out_buf[(y * fy + step) * stride..][..stride].copy_from_slice(&row);
            }
        }
        Some(out)
    };

    if (src_width, src_height) == (width, height) {
        return None;
    }
    if width < src_width || height < src_height {
        return Some(imageops::resize(src, width, height, FilterType::Triangle));
    }

    replicate().or_else(|| Some(imageops::resize(src, width, height, FilterType::Nearest)))
}
