use crate::diagnostics::Diagnostics;
use crate::direction::Quadrant;
use crate::resource::ResourceId;
use crate::resource::resource_manager::ResourceManager;
use crate::resource::texture::TextureSlot;
use crate::resource::texture::cache::TextureCache;
use crate::resource::tint::{ColorMap, TintSource};
use crate::util;
use image::{Rgba, RgbaImage};
use std::borrow::Cow;
use std::str::FromStr;
use std::sync::Arc;

pub struct Layer {
    pub sprite: Arc<RgbaImage>,
    /// u32::MAX = no tint
    pub tint: u32,
}

#[inline(always)]
fn multiply(texels: &mut [u8], tint: u32) {
    if tint == u32::MAX {
        return;
    }
    let [ta, tr, tg, tb] = tint.to_be_bytes();
    let multiply = |ch: u8, t: u8| ((ch as u16 * t as u16) / 255) as u8;
    // ugly but llvm vectorizes this to 32 pixels per iteration on znver4
    // https://godbolt.org/z/Txvsevzav
    for texel in texels.as_chunks_mut::<4>().0 {
        let [r, g, b, a] = *texel;
        *texel = [multiply(r, tr), multiply(g, tg), multiply(b, tb), multiply(a, ta)]
    }
}

#[inline(always)]
fn multiply_texel(texel: [u8; 4], tint: u32) -> [u8; 4] {
    if tint == u32::MAX {
        return texel;
    }
    let [ta, tr, tg, tb] = tint.to_be_bytes();
    let [r, g, b, a] = texel;
    let multiply = |ch: u8, t: u8| ((ch as u16 * t as u16) / 255) as u8;
    [multiply(r, tr), multiply(g, tg), multiply(b, tb), multiply(a, ta)]
}

#[inline(always)]
fn overlay(src: Rgba<u8>, dst: Rgba<u8>) -> Rgba<u8> {
    let src_alpha = src.0[3] as u32;
    if src_alpha == 255 {
        return src;
    }
    if src_alpha == 0 {
        return dst;
    }
    let bg_alpha = dst.0[3] as u32 * (255 - src_alpha) / 255;
    let alpha = src_alpha + bg_alpha;
    if alpha == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let blend = |src: u8, dst: u8| ((src as u32 * src_alpha + dst as u32 * bg_alpha) / alpha) as u8;
    Rgba([
        blend(src.0[0], dst.0[0]),
        blend(src.0[1], dst.0[1]),
        blend(src.0[2], dst.0[2]),
        alpha as u8,
    ])
}

pub async fn layers(
    rm: &ResourceManager,
    textures: &TextureCache,
    slots: &[TextureSlot],
    tints: &[TintSource],
    grass: &ColorMap,
    diag: &Diagnostics,
) -> Vec<Layer> {
    let mut layers = Vec::with_capacity(slots.len());
    let tints = tints.iter().chain(std::iter::repeat(&TintSource::None)).take(slots.len());
    for (slot, tint) in slots.iter().zip(tints) {
        let Ok(id) = ResourceId::from_str(&slot.sprite) else {
            diag.error(&slot.sprite, || format!("Invalid resource id: {}", slot.sprite));
            continue;
        };
        let Ok(sprite) = textures.rendered(rm, &id, Quadrant::R0, (0.0, 0.0), (1.0, 1.0)).await else {
            continue;
        };
        layers.push(Layer { sprite, tint: tint.color_item(grass).unwrap_or(u32::MAX) })
    }
    layers
}

pub fn composite(layers: &[Layer], size: u32) -> Option<RgbaImage> {
    fn fit(sprite: &RgbaImage, width: u32, height: u32) -> Cow<'_, RgbaImage> {
        match util::image::scale(sprite, width, height) {
            Some(scaled) => Cow::Owned(scaled),
            None => Cow::Borrowed(sprite),
        }
    }

    let (first, rest) = layers.split_first()?;
    let (width, height) = rest.iter().fold(first.sprite.dimensions(), |(w, h), layer| {
        let (lw, lh) = layer.sprite.dimensions();
        (w.max(lw), h.max(lh))
    });

    let mut out = fit(&first.sprite, width, height).into_owned();
    if first.tint != u32::MAX {
        multiply(out.as_mut(), first.tint);
    }
    for layer in rest {
        let sprite = fit(&layer.sprite, width, height);
        for (dst, src) in out.pixels_mut().zip(sprite.as_raw().as_chunks::<4>().0) {
            *dst = overlay(Rgba(multiply_texel(*src, layer.tint)), *dst);
        }
    }

    Some(util::image::scale(&out, size, size).unwrap_or(out))
}

pub async fn render(
    rm: &ResourceManager,
    textures: &TextureCache,
    slots: &[TextureSlot],
    tints: &[TintSource],
    grass: &ColorMap,
    size: u32,
    diag: &Diagnostics,
) -> Option<RgbaImage> {
    composite(&layers(rm, textures, slots, tints, grass, diag).await, size)
}
