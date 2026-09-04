use crate::direction::{Direction, Quadrant};
use serde::de::{IntoDeserializer, value};
use serde::{Deserialize, Deserializer};
use ultraviolet::Vec3;

#[derive(Debug, Deserialize)]
pub struct RawFace {
    pub texture: String,
    pub uv: Option<[f32; 4]>,
    #[serde(
        rename = "cullface",
        default,
        deserialize_with = "deserialize_lenient_cull_face"
    )]
    pub cull_face: Option<Direction>,
    #[serde(default)]
    pub rotation: Quadrant,
    #[serde(rename = "tintindex")]
    pub tint_index: Option<i32>,
}

/// mojang handles unknown face names as dont cull instead of throwing an error
fn deserialize_lenient_cull_face<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Direction>, D::Error> {
    let name = <&str>::deserialize(deserializer)?;
    Ok(Direction::deserialize(IntoDeserializer::<value::Error>::into_deserializer(name)).ok())
}

#[derive(Debug)]
pub struct UnresolvedTexture {
    pub texture_ref: String,
    pub rotation: Quadrant,
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
}

impl UnresolvedTexture {
    pub fn from_raw(raw: RawFace, from: &Vec3, to: &Vec3, direction: Direction) -> Self {
        let (from_x, from_y, to_x, to_y) = match raw.uv {
            Some([u0, v0, u1, v1]) => (u0 / 16.0, v0 / 16.0, u1 / 16.0, v1 / 16.0),
            None => UnresolvedTexture::get_dynamic_uv(from, to, direction),
        };
        Self {
            texture_ref: raw.texture,
            rotation: raw.rotation,
            from_x,
            from_y,
            to_x,
            to_y,
        }
    }

    pub fn slot_key(&self) -> &str {
        self.texture_ref.strip_prefix('#').unwrap_or(&self.texture_ref)
    }

    /// Minecraft's default face UVs when a face omits `uv` ([FaceBakery.defaultFaceUV](https://mcsrc.dev/2/26.2/net/minecraft/client/resources/model/cuboid/FaceBakery#L30))
    ///
    /// Every face samples the element's extent in the two axes perpendicular to its own normal. `from`
    /// and `to` are already normalized to `[0, 1]`, so the mirror is `1-c`.
    ///
    /// | Face    | `u`       | `v`       |
    /// |---------|-----------|-----------|
    /// | `Down`  | `x`       | `1 - z`   |
    /// | `Up`    | `x`       | `z`       |
    /// | `North` | `1 - x`   | `1 - y`   |
    /// | `South` | `x`       | `1 - y`   |
    /// | `West`  | `z`       | `1 - y`   |
    /// | `East`  | `1 - z`   | `1 - y`   |
    ///
    /// `v` is mirrored on every face except `Up` because texture `v` grows downward while
    /// world `y`/`z` grow up/north; `u` is mirrored on `North` and `East` so the texture
    /// isn't seen backwards from outside the block.
    fn get_dynamic_uv(from: &Vec3, to: &Vec3, direction: Direction) -> (f32, f32, f32, f32) {
        match direction {
            Direction::Down => (from.x, 1.0 - to.z, to.x, 1.0 - from.z),
            Direction::Up => (from.x, from.z, to.x, to.z),
            Direction::North => (1.0 - to.x, 1.0 - to.y, 1.0 - from.x, 1.0 - from.y),
            Direction::South => (from.x, 1.0 - to.y, to.x, 1.0 - from.y),
            Direction::West => (from.z, 1.0 - to.y, to.z, 1.0 - from.y),
            Direction::East => (1.0 - to.z, 1.0 - to.y, 1.0 - from.z, 1.0 - from.y),
        }
    }
}
