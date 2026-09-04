use super::quad::{display_linear, normal_matrix};
use crate::direction::Direction;
use crate::resource::model::{GuiLight, Transform};
use crate::resource::tint::{ColorMap, TintSource};
use ultraviolet::Vec3;

/// Front on flat item Y-flipped light poses. Mojang renders any gui item through a model view that
/// flips y for shading. These constants are calculated from Mojang's [Lighting](https://mcsrc.dev/2/26.2/com/mojang/blaze3d/platform/Lighting) class:
/// ```
/// D0 = normalize( 0.2, 1.0, -0.7)
/// D1 = normalize(-0.2, 1.0,  0.7)
/// ```
/// joml's normalize is
/// ```java
/// new = old * Math.invsqrt(Math.fma(x, x, Math.fma(y, y, z * z)));
/// ```
/// so
/// ```
/// scalar = 1/sqrt(x^2 + y^2 + z^2)
///        = 1/sqrt(0.2^2 + 1.0^2 + 0.7^2)
///        = 1/sqrt(1.53)
///
/// D0 = ( 0.2 * scalar, 1.0 * scalar, 0.7 * scalar )
/// D0 ~ ( 0.161690, 0.808452, -0.565916)
/// D1 ~ (-0.161690, 0.808452,  0.565916)
/// ```
/// Joml's axis rotation matrices:
/// ```
/// Rx(θ) = |     1      0      0 |
///         |     0   cosθ  -sinθ |
///         |     0   sinθ   cosθ |
///
/// Ry(θ) = |  cosθ      0   sinθ |
///         |     0      1      0 |
///         | -sinθ      0   cosθ |
/// ```
/// so for ITEMS_FLAT
/// ```
/// flatPose = Ry(-π/8) · Rx(3π/4)
///
/// ITEMS_FLAT[0] = flatPose * D0
///               ~ (-0.22252, 0.17150, 0.95973)
///
/// ITEMS_FLAT[1] = flatPose * D1
///               ~ (-0.21501, 0.97183, 0.09657)
/// ```
#[rustfmt::skip]
pub const ITEMS_FLAT: [Vec3; 2] = [
    Vec3 { x: -0.22252, y: 0.17150, z: 0.95973 },
    Vec3 { x: -0.21501, y: 0.97183, z: 0.09657 },
];

/// 3D Y-flipped light poses. Mojang renders any gui item through a model view that flips y. These
/// constants are calculated from Mojang's [Lighting](https://mcsrc.dev/2/26.2/com/mojang/blaze3d/platform/Lighting) class:<br>
/// Joml's axis rotation and scale matrices:
/// ```
/// Rx(θ)    = |     1      0      0 |
///            |     0   cosθ  -sinθ |
///            |     0   sinθ   cosθ |
///
/// Ry(θ)    = |  cosθ      0   sinθ |
///            |     0      1      0 |
///            | -sinθ      0   cosθ |
///
/// S(x,y,z) = |     x      0      0 |
///            |     0      y      0 |
///            |     0      0      z |
///
/// S  = S(1.0, -1.0, 1.0)
/// R1 = Ry(rad(62)) * Rx(rad(185.5))
/// R2 = Ry(-π/8) · Rx(3π/4)
///
/// ITEMS_3D[0] = S * R1 * R2 * D0
///             ~ (-0.93344, 0.26269, -0.24430)
///
/// ITEMS_3D[1] = S * R1 * R2 * D1
///             ~ (-0.10357, 0.97661, 0.18845)
/// ```
/// D0 and D1 calculated in [`ITEMS_FLAT`]s comment
#[rustfmt::skip]
pub const ITEMS_3D: [Vec3; 2] = [
    Vec3 { x: -0.93344, y: 0.26269, z: -0.24430 },
    Vec3 { x: -0.10357, y: 0.97661, z: 0.18845 },
];

#[inline]
pub fn shade_for(normal: Vec3, lights: &[Vec3; 2]) -> f32 {
    (0.6 * (lights[0].dot(normal).max(0.0) + lights[1].dot(normal).max(0.0)) + 0.4).min(1.0)
}

pub fn fixed_factors(shade: f32, tint: u32) -> [u32; 3] {
    let [_, tr, tg, tb] = tint.to_be_bytes();
    let fix = |ch: u8| (ch as f32 * shade * 65536.0 / 255.0) as u32;
    [fix(tr), fix(tg), fix(tb)]
}

pub fn shade_table(display: &Transform, light: GuiLight) -> [f32; 6] {
    let lights = match light {
        GuiLight::Side => &ITEMS_3D,
        GuiLight::Front => &ITEMS_FLAT,
    };
    let mat = normal_matrix(display_linear(display));
    Direction::ALL.map(|dir| shade_for((mat * dir.unit()).normalized(), lights))
}

pub fn tint_table(sources: &[TintSource], grass: &ColorMap) -> Vec<u32> {
    sources
        .iter()
        .map(|tint| tint.color_item(grass).unwrap_or(u32::MAX))
        .collect()
}
