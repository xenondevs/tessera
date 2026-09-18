use super::quad::ScreenQuad;
use super::shade::fixed_factors;
use crate::resource::texture::sprite::Blend;
use image::RgbaImage;

/// One texel in fixed-point 32.32 format
const FIXED_ONE: f64 = (1u64 << 32) as f64;

pub struct Target {
    pub width: u32,
    pub height: u32,
    pub color: Vec<u32>,
    pub depth: Vec<f32>,
}

impl Target {
    pub fn new(width: u32, height: u32, depth: bool) -> Self {
        Self {
            width,
            height,
            color: vec![0; (width * height) as usize],
            depth: if depth {
                vec![f32::INFINITY; (width * height) as usize]
            } else {
                Vec::new()
            },
        }
    }

    pub fn clear(&mut self) {
        self.color.fill(0);
        self.depth.fill(f32::INFINITY);
    }

    pub fn into_image(self) -> RgbaImage {
        // SAFETY: u32 is obviously aligned for u8
        let bytes = unsafe { std::slice::from_raw_parts(self.color.as_ptr().cast::<u8>(), self.color.len() * 4) };
        RgbaImage::from_raw(self.width, self.height, bytes.to_vec()).expect("buffer is width * height")
    }
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

fn rasterize<P: Pass, C: Coverage, const DEPTH: bool>(out: &mut Target, shaded: &mut Vec<u32>, quad: &ScreenQuad) {
    let tex = &*quad.sprite;
    let bounds @ (min_x, min_y, max_x, max_y) = bbox(quad, out.width, out.height);
    if min_x >= max_x || min_y >= max_y {
        return;
    }
    let color_factors = fixed_factors(quad.shade, quad.tint);
    let source = tex.image.as_chunks::<4>().0;
    if (max_x - min_x) * (max_y - min_y) >= source.len() as i32 {
        shaded.clear();
        shaded.extend(
            source
                .iter()
                .map(|texel| modulate(u32::from_le_bytes(*texel), color_factors)),
        );
        fill::<P, C, DEPTH, true>(out, shaded, quad, color_factors, bounds);
    } else {
        fill::<P, C, DEPTH, false>(out, &[], quad, color_factors, bounds);
    }
}

fn fill<P: Pass, C: Coverage, const DEPTH: bool, const SHADED: bool>(
    out: &mut Target,
    shaded: &[u32],
    quad: &ScreenQuad,
    color_factors: [u32; 3],
    (min_x, min_y, max_x, max_y): (i32, i32, i32, i32),
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

    let tex = &*quad.sprite;
    let source = tex.image.as_chunks::<4>().0;
    let [ds_dx, ds_dy, dt_dx, dt_dy] = quad.inverse;
    let (tex_w, tex_h) = (tex.width as f32, tex.height as f32);
    let (du_ds, dv_ds) = (quad.uv_gradient[0][0] as f64, quad.uv_gradient[0][1] as f64);
    let (du_dt, dv_dt) = (quad.uv_gradient[1][0] as f64, quad.uv_gradient[1][1] as f64);
    let (u_origin, v_origin) = (quad.uv_origin[0] as f64, quad.uv_origin[1] as f64);

    let du_dx = ((du_ds * ds_dx as f64 + du_dt * dt_dx as f64) * tex_w as f64 * FIXED_ONE) as i64;
    let dv_dx = ((dv_ds * ds_dx as f64 + dv_dt * dt_dx as f64) * tex_h as f64 * FIXED_ONE) as i64;
    let (last_texel_x, last_texel_y) = (tex.width as i64 - 1, tex.height as i64 - 1);
    let stride = tex.width as usize;
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
        let mut tex_u = ((u_origin + s as f64 * du_ds + t as f64 * du_dt) * tex_w as f64 * FIXED_ONE) as i64;
        let mut tex_v = ((v_origin + s as f64 * dv_ds + t as f64 * dv_dt) * tex_h as f64 * FIXED_ONE) as i64;
        let row_offset = (y as u32 * out.width) as usize;

        for x in start..min_x + hi {
            if C::inside(s, t) {
                let depth = quad.origin_depth + s * quad.depth_gradient[0] + t * quad.depth_gradient[1];
                let pixel = row_offset + x as usize;
                if !DEPTH || P::depth_passes(depth, out.depth[pixel]) {
                    let texel_x = (tex_u >> 32).clamp(0, last_texel_x) as usize;
                    let texel_y = (tex_v >> 32).clamp(0, last_texel_y) as usize;
                    let texel = texel_y * stride + texel_x;
                    let src = if SHADED {
                        *unsafe { shaded.get_unchecked(texel) }
                    } else {
                        let raw = *unsafe { source.get_unchecked(texel) };
                        modulate(u32::from_le_bytes(raw), color_factors)
                    };
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
            tex_u += du_dx;
            tex_v += dv_dx;
        }
    }
}

fn draw(out: &mut Target, shaded: &mut Vec<u32>, quad: &ScreenQuad, depth: bool) {
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

pub fn render(out: &mut Target, quads: &mut [ScreenQuad], depth: bool) {
    let near = |quad: &ScreenQuad| quad.origin_depth + quad.depth_gradient[0].min(0.0) + quad.depth_gradient[1].min(0.0);
    let far = |q: &ScreenQuad| q.origin_depth + q.depth_gradient[0].max(0.0) + q.depth_gradient[1].max(0.0);
    let rank = |q: &ScreenQuad| match q.pass {
        PassKind::Opaque => 0,
        PassKind::Cutout(_) => 1,
        PassKind::Translucent => 2,
    };
    quads.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then_with(|| match a.pass {
            PassKind::Translucent => far(b).total_cmp(&far(a)).then_with(|| near(a).total_cmp(&near(b))),
            _ => near(a).total_cmp(&near(b)),
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
