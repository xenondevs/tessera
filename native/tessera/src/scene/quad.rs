use crate::direction::Direction;
use crate::direction::Quadrant;
use crate::resource::blockstate::ModelState;
use crate::resource::model::Transform;
use crate::resource::model::element::{Element, Face, ShadeDirection};
use crate::resource::texture::sprite::{Sprite, missing_sprite};
use crate::util::FastHashMap;
use std::sync::Arc;
use ultraviolet::{Mat3, Vec3};

/// Corner index bits: bit-0 = x, bit-1 = y, bit-2 = z
/// todo, change to Direction::ALL map once https://github.com/rust-lang/rust/issues/143874 is stable
const FACE_CORNERS: [[usize; 4]; 6] = [
    [3, 1, 0, 2], // north
    [7, 5, 1, 3], // east
    [6, 4, 5, 7], // south
    [2, 0, 4, 6], // west
    [2, 6, 7, 3], // up
    [4, 0, 1, 5], // down
];

const SUBPIXEL: f32 = 256.0;

#[derive(Clone, Debug)]
pub struct ParaQuad {
    pub origin: [f32; 2],
    pub edges: [[f32; 2]; 2],
    pub inverse: [f32; 4],
    pub origin_depth: f32,
    pub depth_gradient: [f32; 2],
    pub uv_origin: [f32; 2],
    pub uv_gradient: [[f32; 2]; 2],
    pub sprite: Arc<Sprite>,
    pub light_emission: u8,
    pub tint: u32,
    pub shade: Direction,
}

pub struct Affine {
    pub linear: Mat3,
    pub translation: Vec3,
}

impl Affine {
    const fn identity() -> Self {
        Self {
            linear: Mat3::new(
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ),
            translation: Vec3::new(0.0, 0.0, 0.0),
        }
    }

    pub fn apply(&self, v: Vec3) -> Vec3 {
        self.linear * v + self.translation
    }

    fn after(&self, rhs: &Affine) -> Affine {
        Affine {
            linear: self.linear * rhs.linear,
            translation: self.linear * rhs.translation + self.translation,
        }
    }

    fn about(lin_mat: Mat3, pivot: Vec3) -> Affine {
        Affine { linear: lin_mat, translation: pivot - lin_mat * pivot }
    }
}

const fn cos_sin(q: Quadrant) -> (f32, f32) {
    match q {
        Quadrant::R0 => (1.0, 0.0),
        Quadrant::R90 => (0.0, -1.0),
        Quadrant::R180 => (-1.0, 0.0),
        Quadrant::R270 => (0.0, 1.0),
    }
}

const fn rot_x(q: Quadrant) -> Mat3 {
    let (c, s) = cos_sin(q);
    Mat3::new(
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, c, s),
        Vec3::new(0.0, -s, c),
    )
}

const fn rot_y(q: Quadrant) -> Mat3 {
    let (c, s) = cos_sin(q);
    Mat3::new(
        Vec3::new(c, 0.0, -s),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(s, 0.0, c),
    )
}

const fn rot_z(q: Quadrant) -> Mat3 {
    let (c, s) = cos_sin(q);
    Mat3::new(
        Vec3::new(c, s, 0.0),
        Vec3::new(-s, c, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    )
}

/// [order ref](https://mcsrc.dev/2/26.2/com/mojang/math/Quadrant#L57)
fn state_matrix(state: &ModelState) -> Mat3 {
    rot_z(state.z) * rot_y(state.y) * rot_x(state.x)
}

/// [ref](https://mcsrc.dev/2/26.2/net/minecraft/client/resources/model/cuboid/ItemTransform#L38-41)
pub fn display_linear(t: &Transform) -> Mat3 {
    let rot = Mat3::from_rotation_x(t.rotation.x.to_radians())
        * Mat3::from_rotation_y(t.rotation.y.to_radians())
        * Mat3::from_rotation_z(t.rotation.z.to_radians());
    rot * Mat3::from_nonuniform_scale(t.scale)
}

fn closest_direction(vec: Vec3) -> Option<Direction> {
    const NEAREST_ORDER: [Direction; 6] = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];

    if !vec.x.is_finite() || !vec.y.is_finite() || !vec.z.is_finite() {
        return None;
    }
    let mut best = None;
    let mut closest = 0f32;
    for (d, unit) in NEAREST_ORDER.iter().map(|d| (d, d.unit())) {
        let dot = vec.dot(unit);
        if dot >= 0.0 && dot > closest {
            closest = dot;
            best = Some(*d);
        }
    }
    best
}

pub fn normal_matrix(mat: Mat3) -> Mat3 {
    let det = mat.determinant();
    if det.abs() < 1e-9 {
        Mat3::identity()
    } else {
        mat.inversed().transposed()
    }
}

/// [ref](https://mcsrc.dev/1/26.2/net/minecraft/core/BlockMath#L13)
fn uv_local_to_global(dir: Direction) -> Mat3 {
    const H: f32 = std::f32::consts::FRAC_PI_2;
    match dir {
        Direction::North => Mat3::from_rotation_y(std::f32::consts::PI),
        Direction::East => Mat3::from_rotation_y(H),
        Direction::South => Mat3::identity(),
        Direction::West => Mat3::from_rotation_y(-H),
        Direction::Up => Mat3::from_rotation_x(-H),
        Direction::Down => Mat3::from_rotation_x(H),
    }
}

fn uv_lock_matrix(state_mat: Mat3, face: Direction) -> Option<Mat3> {
    let action = state_mat * uv_local_to_global(face);
    let new_side = closest_direction(action * Vec3::unit_z())?;
    Some((uv_local_to_global(new_side).inversed() * action).inversed())
}

/// [order ref](https://mcsrc.dev/2/26.2/net/minecraft/client/resources/model/cuboid/CuboidFace#L76)
/// (min, min), (min, max), (max, max), (max, min)
fn uv_corners(face: &Face, lock: Option<Mat3>) -> [[f32; 2]; 4] {
    let [u0, v0, u1, v1] = face.uv;
    let mut corners = [[u0, v0], [u0, v1], [u1, v1], [u1, v0]];
    if let Some(lock) = lock {
        for corner in corners.iter_mut() {
            let uv = lock * Vec3::new(corner[0] - 0.5, corner[1] - 0.5, 0.0);
            *corner = [uv.x + 0.5, uv.y + 0.5];
        }
    }
    corners
}

pub fn project(
    elements: &[Element],
    textures: &FastHashMap<String, Arc<Sprite>>,
    tints: &[u32],
    state: &ModelState,
    display: &Transform,
    size: u32,
    out: &mut Vec<ParaQuad>,
) {

    let size = size as f32;
    let center = Vec3::broadcast(0.5);
    let state_mat = state_matrix(state);

    let display_mat = display_linear(display);
    let to_gui = Affine {
        linear: display_mat,
        translation: display.translation - display_mat * center,
    };
    let world = to_gui.after(&Affine::about(state_mat, center));

    let locks: [Option<Mat3>; 6] = std::array::from_fn(|i| {
        state
            .uv_lock
            .then(|| uv_lock_matrix(state_mat, Direction::ALL[i]))
            .flatten()
    });

    for el in elements {
        let elem_aff = match &el.rotation {
            Some(rot) => Affine::about(rot.matrix, rot.origin),
            None => Affine::identity(),
        };
        let mat = world.after(&elem_aff);

        let corners: [[f32; 3]; 8] = std::array::from_fn(|i| {
            let v = Vec3::new(
                if i & 1 == 0 { el.from.x } else { el.to.x },
                if i & 2 == 0 { el.from.y } else { el.to.y },
                if i & 4 == 0 { el.from.z } else { el.to.z },
            );
            let point = mat.apply(v);
            let snap = |v: f32| (v * SUBPIXEL).round() / SUBPIXEL;
            [
                snap((0.5 + point.x) * size),
                snap((0.5 - point.y) * size),
                -point.z,
            ]
        });

        let normal_mat = normal_matrix(state_mat * elem_aff.linear);
        for (i, face) in el.faces.iter().enumerate() {
            let Some(face) = face else { continue };

            let facing = Direction::ALL[i];
            let slots = FACE_CORNERS[i];

            let shift = face.rotation as usize;
            let origin_slot = (4 - shift) % 4;
            let corner_origin = corners[slots[origin_slot]]; // uv (0,0) after rotation
            let corner_u = corners[slots[(origin_slot + 3) % 4]]; // uv (1,0)
            let corner_v = corners[slots[(origin_slot + 1) % 4]]; // uv (0,1)

            let edge_u = [corner_u[0] - corner_origin[0], corner_u[1] - corner_origin[1]];
            let edge_v = [corner_v[0] - corner_origin[0], corner_v[1] - corner_origin[1]];
            let det = edge_u[0] * edge_v[1] - edge_u[1] * edge_v[0];

            if det <= 0.0 {
                continue;
            }

            let inv_det = 1.0 / det;
            let uv = uv_corners(face, locks[i]);
            let (co, cu, cv) = (uv[0], uv[3], uv[1]);

            let shade = match el.shade_direction {
                ShadeDirection::Override(dir) => dir,
                ShadeDirection::Actual => {
                    closest_direction(normal_mat * facing.unit()).unwrap_or(Direction::Up)
                }
            };

            out.push(ParaQuad {
                origin: [corner_origin[0], corner_origin[1]],
                edges: [edge_u, edge_v],
                inverse: [
                    edge_v[1] * inv_det,
                    -edge_v[0] * inv_det,
                    -edge_u[1] * inv_det,
                    edge_u[0] * inv_det,
                ],
                origin_depth: corner_origin[2],
                depth_gradient: [corner_u[2] - corner_origin[2], corner_v[2] - corner_origin[2]],
                uv_origin: co,
                uv_gradient: [[cu[0] - co[0], cu[1] - co[1]], [cv[0] - co[0], cv[1] - co[1]]],
                sprite: textures.get(&face.texture).unwrap_or(missing_sprite()).clone(),
                light_emission: el.light_emission,
                tint: usize::try_from(face.tint_index)
                    .ok()
                    .and_then(|i| tints.get(i).copied())
                    .unwrap_or(u32::MAX),
                shade,
            });
        }
    }
}
