use super::ModelError;
use super::raw::{RawElement, RawRotation};
use crate::direction::{Axis, Direction, Quadrant};
use crate::resource::texture::face::UnresolvedTexture;
use ultraviolet::{Mat3, Vec3};

pub struct Element {
    /// Normalized 0..1
    pub from: Vec3,
    /// Normalized 0..1
    pub to: Vec3,
    pub faces: [Option<Face>; 6],
    pub rotation: Option<ElementRotation>,
    pub shade_direction: ShadeDirection,
    pub light_emission: u8,
}

impl TryFrom<RawElement> for Element {
    type Error = ModelError;

    fn try_from(value: RawElement) -> Result<Self, Self::Error> {
        const MIN_EXTENT: f32 = -16.0;
        const MAX_EXTENT: f32 = 32.0;

        let bounded = |v: [f32; 3]| v.iter().all(|c| (MIN_EXTENT..=MAX_EXTENT).contains(c));

        if !bounded(value.from) || !bounded(value.to) {
            return Err(ModelError::ExtentOutOfBounds { from: value.from, to: value.to });
        }
        if value.faces.iter().all(|f| f.is_none()) {
            return Err(ModelError::NoFaces);
        }

        let light_emission = u8::try_from(value.light_emission)
            .ok()
            .filter(|v| *v <= 15)
            .ok_or(ModelError::LightEmission(value.light_emission))?;

        let shade_direction = value
            .shade_direction_override
            .map(ShadeDirection::Override)
            .unwrap_or(ShadeDirection::Actual);

        let from = Vec3::from(value.from) / 16.0;
        let to = Vec3::from(value.to) / 16.0;
        let mut faces: [Option<Face>; 6] = Default::default();
        for (face, dst, dir) in value
            .faces
            .into_iter()
            .zip(faces.iter_mut())
            .zip(Direction::ALL.iter())
            .map(|((face, dst), dir)| (face, dst, dir))
        {
            let Some(raw) = face else { continue };
            let tint_index = raw.tint_index.unwrap_or(-1);
            let texture = UnresolvedTexture::from_raw(raw, &from, &to, *dir);
            *dst = Some(Face {
                uv: [texture.from_x, texture.from_y, texture.to_x, texture.to_y],
                rotation: texture.rotation,
                tint_index,
                texture: texture.slot_key().to_owned(),
            });
        }

        Ok(Element {
            from,
            to,
            faces,
            rotation: value.rotation.map(ElementRotation::build).transpose()?,
            shade_direction,
            light_emission,
        })
    }
}

pub enum ShadeDirection {
    Actual,
    Override(Direction),
}

pub struct ElementRotation {
    pub origin: Vec3,
    /// Rotation + scale
    pub matrix: Mat3,
}

impl ElementRotation {
    fn build(raw: RawRotation) -> Result<Self, ModelError> {
        let value = if raw.axis.is_none() && raw.angle.is_none() {
            if raw.x.is_none() && raw.y.is_none() && raw.z.is_none() {
                return Err(ModelError::MissingRotationValue);
            }
            RotationValue::Euler {
                x: raw.x.unwrap_or(0.0),
                y: raw.y.unwrap_or(0.0),
                z: raw.z.unwrap_or(0.0),
            }
        } else {
            RotationValue::SingleAxis {
                axis: raw.axis.ok_or(ModelError::MissingAxis)?,
                angle: raw.angle.ok_or(ModelError::MissingAngle)?,
            }
        };
        let rot = rotation_matrix(&value);
        let matrix = if raw.rescale && rot != Mat3::identity() {
            apply_rescale(rot)
        } else {
            rot
        };
        Ok(ElementRotation { origin: Vec3::from(raw.origin) / 16.0, matrix })
    }
}

enum RotationValue {
    SingleAxis { axis: Axis, angle: f32 },
    Euler { x: f32, y: f32, z: f32 },
}

fn rotation_matrix(value: &RotationValue) -> Mat3 {
    match *value {
        RotationValue::SingleAxis { angle: 0.0, .. } => Mat3::identity(),
        RotationValue::SingleAxis { axis, angle } => {
            let r = angle.to_radians();
            match axis {
                Axis::X => Mat3::from_rotation_x(r),
                Axis::Y => Mat3::from_rotation_y(r),
                Axis::Z => Mat3::from_rotation_z(r),
            }
        }
        RotationValue::Euler { x, y, z } => {
            let rx = x.to_radians();
            let ry = y.to_radians();
            let rz = z.to_radians();
            Mat3::from_rotation_z(rz) * Mat3::from_rotation_y(ry) * Mat3::from_rotation_x(rx)
        }
    }
}

fn apply_rescale(rot: Mat3) -> Mat3 {
    let factor = |u: Vec3| {
        let t = rot * u;
        1.0 / t.x.abs().max(t.y.abs()).max(t.z.abs())
    };
    let scale = Vec3::new(
        factor(Vec3::unit_x()),
        factor(Vec3::unit_y()),
        factor(Vec3::unit_z()),
    );

    rot * Mat3::from_nonuniform_scale(scale)
}

pub struct Face {
    /// normalized min U, min V, max U, max V
    pub uv: [f32; 4],
    pub rotation: Quadrant,
    pub tint_index: i32,
    pub texture: String,
}
