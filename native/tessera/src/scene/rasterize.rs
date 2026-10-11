use super::end_portal::EndPortal;
use super::quad::ScreenQuad;
use super::shade::fixed_factors;
use crate::resource::texture::sprite::Blend;
use image::RgbaImage;
use std::sync::Arc;

/// One texel in fixed-point 32.32 format
const FIXED_ONE: f64 = (1u64 << 32) as f64;

pub struct Canvas<'a> {
    pub width: u32,
    pub height: u32,
    pub color: &'a mut [u32],
    pub depth: &'a mut [f32],
}

pub fn render_image(width: u32, height: u32, quads: &mut [ScreenQuad], depth: bool) -> RgbaImage {
    let len = (width * height) as usize;
    let mut depth_buffer = if depth { vec![f32::INFINITY; len] } else { Vec::new() };
    let mut image = RgbaImage::new(width, height);

    // SAFETY: u32 is obviously aligned for u8
    let (before, color, after) = unsafe { image.align_to_mut::<u32>() };
    if before.is_empty() && after.is_empty() {
        render(&mut Canvas { width, height, color, depth: &mut depth_buffer }, quads, depth);
    } else {
        let mut pixels = vec![0u32; len];
        let mut canvas = Canvas { width, height, color: &mut pixels, depth: &mut depth_buffer };
        render(&mut canvas, quads, depth);
        for (bytes, pixel) in image.as_chunks_mut::<4>().0.iter_mut().zip(&pixels) {
            *bytes = pixel.to_ne_bytes();
        }
    }

    image
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PassKind {
    Opaque,
    Cutout(AlphaCutout),
    Translucent,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AlphaCutout {
    /// < 1 is invisible (255)
    Visible,
    /// < 0.5 is invisible (128)
    Half,
    /// < 0.1 is invisible (26)
    Tenth,
}

impl From<Blend> for PassKind {
    fn from(value: Blend) -> Self {
        match value {
            Blend::Opaque => Self::Opaque,
            Blend::Cutout => Self::Cutout(AlphaCutout::Visible),
            Blend::Translucent => Self::Translucent,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum QuadKind {
    Parallelogram,
    Triangle,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
}

#[derive(Clone, Debug)]
pub enum Shader {
    Sprite,
    EndPortal(Arc<EndPortal>),
}

trait Pass {
    const WRITES_DEPTH: bool;
    fn depth_passes(depth: f32, stored: f32) -> bool;
    fn keep(src: u32) -> bool;
    fn compose(src: u32, dst: u32) -> u32;
}

struct Opaque;
struct Cutout<const MIN: u32>;
struct Translucent;

impl Pass for Opaque {
    const WRITES_DEPTH: bool = true;

    #[inline(always)]
    fn depth_passes(depth: f32, stored: f32) -> bool {
        depth < stored
    }

    #[inline(always)]
    fn keep(_: u32) -> bool {
        true
    }

    #[inline(always)]
    fn compose(src: u32, _: u32) -> u32 {
        src
    }
}

impl<const MIN: u32> Pass for Cutout<MIN> {
    const WRITES_DEPTH: bool = true;

    #[inline(always)]
    fn depth_passes(depth: f32, stored: f32) -> bool {
        depth < stored
    }

    #[inline(always)]
    fn keep(src: u32) -> bool {
        src >> 24 >= MIN
    }

    #[inline(always)]
    fn compose(src: u32, _: u32) -> u32 {
        src
    }
}

impl Pass for Translucent {
    const WRITES_DEPTH: bool = false;

    // nearer-or-equal for stuff directly drawn on top of each other (e.g. a banner pattern)
    #[inline(always)]
    fn depth_passes(depth: f32, stored: f32) -> bool {
        depth <= stored
    }

    #[inline(always)]
    fn keep(src: u32) -> bool {
        src & 0xFF000000 != 0
    }

    #[inline(always)]
    fn compose(src: u32, dst: u32) -> u32 {
        over(src, dst)
    }
}

trait Coverage {
    fn inside(s: f32, t: f32) -> bool;
}

struct Parallelogram;
struct Triangle;

impl Coverage for Parallelogram {
    #[inline(always)]
    fn inside(s: f32, t: f32) -> bool {
        ((s - 0.5).abs() <= 0.5) & ((t - 0.5).abs() <= 0.5)
    }
}

impl Coverage for Triangle {
    #[inline(always)]
    fn inside(s: f32, t: f32) -> bool {
        (s >= 0.0) & (t >= 0.0) & (s + t <= 1.0)
    }
}

pub(super) trait Source {
    /// Per-row state stepped per pixel
    type Cursor;

    fn cursor(&self, x: i32, y: i32, s: f32, t: f32) -> Self::Cursor;
    fn step(&self, cursor: &mut Self::Cursor);
    fn color(&self, cursor: &Self::Cursor) -> u32;
}

struct TexturedSource<F> {
    origin: [f64; 2],
    per_s: [f64; 2],
    per_t: [f64; 2],
    size: [f64; 2],
    /// Texels per pixel along a row, in 32.32
    per_x: [i64; 2],
    last: [i64; 2],
    stride: usize,
    texel: F,
}

impl<F: Fn(usize) -> u32> TexturedSource<F> {
    fn new(quad: &ScreenQuad, texel: F) -> Self {
        let tex = &*quad.sprite;
        let [ds_dx, _, dt_dx, _] = quad.inverse.map(f64::from);
        let [per_s, per_t] = quad.uv_gradient.map(|gradient| gradient.map(f64::from));
        let size = [tex.width as f64, tex.height as f64];
        Self {
            origin: quad.uv_origin.map(f64::from),
            per_s,
            per_t,
            size,
            per_x: [0, 1].map(|i| ((per_s[i] * ds_dx + per_t[i] * dt_dx) * size[i] * FIXED_ONE) as i64),
            last: [tex.width as i64 - 1, tex.height as i64 - 1],
            stride: tex.width as usize,
            texel,
        }
    }
}

impl<F: Fn(usize) -> u32> Source for TexturedSource<F> {
    type Cursor = [i64; 2];

    #[inline(always)]
    fn cursor(&self, _: i32, _: i32, s: f32, t: f32) -> Self::Cursor {
        let (s, t) = (s as f64, t as f64);
        [0, 1].map(|i| ((self.origin[i] + s * self.per_s[i] + t * self.per_t[i]) * self.size[i] * FIXED_ONE) as i64)
    }

    #[inline(always)]
    fn step(&self, cursor: &mut Self::Cursor) {
        cursor[0] += self.per_x[0];
        cursor[1] += self.per_x[1];
    }

    fn color(&self, &[u, v]: &Self::Cursor) -> u32 {
        let x = (u >> 32).clamp(0, self.last[0]) as usize;
        let y = (v >> 32).clamp(0, self.last[1]) as usize;
        (self.texel)(y * self.stride + x)
    }
}

#[inline(always)]
fn modulate(texel: u32, k: [u32; 3]) -> u32 {
    let [r, g, b, a] = texel.to_le_bytes();
    u32::from_le_bytes([
        ((r as u32 * k[0] + 0x8000) >> 16) as u8,
        ((g as u32 * k[1] + 0x8000) >> 16) as u8,
        ((b as u32 * k[2] + 0x8000) >> 16) as u8,
        a,
    ])
}

#[inline(always)]
fn over(src: u32, dst: u32) -> u32 {
    let [sr, sg, sb, sa] = src.to_le_bytes();
    let [dr, dg, db, da] = dst.to_le_bytes();
    let sa = sa as u32;
    if sa == 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    let behind = da as u32 * (255 - sa) / 255;
    let alpha = sa + behind;
    if alpha == 0 {
        return 0;
    }
    let blend = |src: u8, dst: u8| ((src as u32 * sa + dst as u32 * behind) / alpha) as u8;
    u32::from_le_bytes([blend(sr, dr), blend(sg, dg), blend(sb, db), alpha as u8])
}

#[inline(always)]
fn bbox(quad: &ScreenQuad, width: u32, height: u32) -> (i32, i32, i32, i32) {
    let xs = [
        quad.origin[0],
        quad.origin[0] + quad.edges[0][0],
        quad.origin[0] + quad.edges[1][0],
        quad.origin[0] + quad.edges[0][0] + quad.edges[1][0],
    ];
    let ys = [
        quad.origin[1],
        quad.origin[1] + quad.edges[0][1],
        quad.origin[1] + quad.edges[1][1],
        quad.origin[1] + quad.edges[0][1] + quad.edges[1][1],
    ];

    let min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min).floor().max(0.0) as i32;
    let min_y = ys.iter().cloned().fold(f32::INFINITY, f32::min).floor().max(0.0) as i32;
    let max_x = xs.into_iter().fold(f32::NEG_INFINITY, f32::max).ceil().min(width as f32) as i32;
    let max_y = ys.into_iter().fold(f32::NEG_INFINITY, f32::max).ceil().min(height as f32) as i32;

    (min_x, min_y, max_x, max_y)
}

fn rasterize<P: Pass, C: Coverage, const DEPTH: bool>(out: &mut Canvas, shaded: &mut Vec<u32>, quad: &ScreenQuad) {
    let bounds @ (min_x, min_y, max_x, max_y) = bbox(quad, out.width, out.height);
    if min_x >= max_x || min_y >= max_y {
        return;
    }
    let color_factors = fixed_factors(quad.shade, quad.tint);
    let source = quad.sprite.image.as_chunks::<4>().0;
    // SAFETY: for both lookups, TexturedSource clamps to the sprite's last texel
    if (max_x - min_x) * (max_y - min_y) >= source.len() as i32 {
        shaded.clear();
        shaded.extend(
            source
                .iter()
                .map(|texel| modulate(u32::from_le_bytes(*texel), color_factors)),
        );
        let lookup = |texel: usize| *unsafe { shaded.get_unchecked(texel) };
        fill::<P, C, DEPTH>(out, quad, bounds, &TexturedSource::new(quad, lookup));
    } else {
        let lookup = |texel: usize| modulate(u32::from_le_bytes(*unsafe { source.get_unchecked(texel) }), color_factors);
        fill::<P, C, DEPTH>(out, quad, bounds, &TexturedSource::new(quad, lookup));
    }
}

fn fill<P: Pass, C: Coverage, const DEPTH: bool>(
    out: &mut Canvas,
    quad: &ScreenQuad,
    (min_x, min_y, max_x, max_y): (i32, i32, i32, i32),
    source: &impl Source,
) {
    fn window(start: f32, step: f32, n: i32) -> (i32, i32) {
        if step == 0.0 {
            return if (0.0..=1.0).contains(&start) { (0, n) } else { (0, 0) };
        }
        let (cross_lo, cross_hi) = (-start / step, (1.0 - start) / step);
        let (enter, exit) = if step > 0.0 {
            (cross_lo, cross_hi)
        } else {
            (cross_hi, cross_lo)
        };
        ((enter as i32 - 1).max(0), (exit as i32 + 2).min(n))
    }

    let [ds_dx, ds_dy, dt_dx, dt_dy] = quad.inverse;
    let width = max_x - min_x;

    for y in min_y..max_y {
        let delta_y = y as f32 + 0.5 - quad.origin[1];
        let base_x = min_x as f32 + 0.5 - quad.origin[0];
        let (s_lo, s_hi) = window(ds_dx * base_x + ds_dy * delta_y, ds_dx, width);
        let (t_lo, t_hi) = window(dt_dx * base_x + dt_dy * delta_y, dt_dx, width);
        let (lo, hi) = (s_lo.max(t_lo), s_hi.min(t_hi));
        if lo >= hi {
            continue;
        }

        let start = min_x + lo;
        let delta_x = start as f32 + 0.5 - quad.origin[0];
        let mut s = ds_dx * delta_x + ds_dy * delta_y;
        let mut t = dt_dx * delta_x + dt_dy * delta_y;
        let mut cursor = source.cursor(start, y, s, t);
        let row_offset = (y as u32 * out.width) as usize;

        for x in start..min_x + hi {
            if C::inside(s, t) {
                let depth = quad.origin_depth + s * quad.depth_gradient[0] + t * quad.depth_gradient[1];
                let pixel = row_offset + x as usize;
                if !DEPTH || P::depth_passes(depth, out.depth[pixel]) {
                    let src = source.color(&cursor);
                    if P::keep(src) {
                        if DEPTH && P::WRITES_DEPTH {
                            out.depth[pixel] = depth;
                        }
                        out.color[pixel] = P::compose(src, out.color[pixel])
                    }
                }
            }
            s += ds_dx;
            t += dt_dx;
            source.step(&mut cursor);
        }
    }
}

fn draw(out: &mut Canvas, shaded: &mut Vec<u32>, quad: &ScreenQuad, depth: bool) {
    if let Shader::EndPortal(portal) = &quad.shader {
        let bounds = bbox(quad, out.width, out.height);
        let source = portal.as_source(out);
        return match depth {
            true => fill::<Opaque, Parallelogram, true>(out, quad, bounds, &source),
            false => fill::<Opaque, Parallelogram, false>(out, quad, bounds, &source),
        };
    }
    macro_rules! with_pass {
        ($pass:ty) => {
            match (quad.kind, depth) {
                (QuadKind::Parallelogram, true) => rasterize::<$pass, Parallelogram, true>(out, shaded, quad),
                (QuadKind::Parallelogram, false) => rasterize::<$pass, Parallelogram, false>(out, shaded, quad),
                (QuadKind::Triangle, true) => rasterize::<$pass, Triangle, true>(out, shaded, quad),
                (QuadKind::Triangle, false) => rasterize::<$pass, Triangle, false>(out, shaded, quad),
            }
        };
    }

    match quad.pass {
        PassKind::Opaque => with_pass!(Opaque),
        PassKind::Cutout(AlphaCutout::Visible) => with_pass!(Cutout<1>),
        PassKind::Cutout(AlphaCutout::Tenth) => with_pass!(Cutout<26>),
        PassKind::Cutout(AlphaCutout::Half) => with_pass!(Cutout<128>),
        PassKind::Translucent => with_pass!(Translucent),
    }
}

pub fn render(out: &mut Canvas, quads: &mut [ScreenQuad], depth: bool) {
    let near = |quad: &ScreenQuad| quad.origin_depth + quad.depth_gradient[0].min(0.0) + quad.depth_gradient[1].min(0.0);
    let far = |q: &ScreenQuad| q.origin_depth + q.depth_gradient[0].max(0.0) + q.depth_gradient[1].max(0.0);
    let rank = |q: &ScreenQuad| match q.pass {
        PassKind::Opaque => 0,
        PassKind::Cutout(_) => 1,
        PassKind::Translucent => 2,
    };
    let side = |q: &ScreenQuad| !depth && q.side == Side::Front;
    quads.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then_with(|| match a.pass {
            PassKind::Translucent => far(b).total_cmp(&far(a)).then_with(|| near(a).total_cmp(&near(b))),
            _ => side(a).cmp(&side(b)).then_with(|| near(a).total_cmp(&near(b))),
        })
    });

    let mut shaded = Vec::new();
    for quad in quads.iter() {
        draw(out, &mut shaded, quad, depth);
    }
}

pub fn needs_depth(part_count: usize, element_count: usize, quads: &[ScreenQuad]) -> bool {
    part_count > 1 || element_count > 1 || quads.iter().any(|q| q.sprite.blend == Blend::Translucent)
}
