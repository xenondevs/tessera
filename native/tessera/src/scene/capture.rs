use crate::capture::lookup::Capture;
use crate::direction::{Direction, Quadrant};
use crate::resource::ResourceId;
use crate::resource::cache::Caches;
use crate::resource::model::Transform;
use crate::resource::texture::sprite::{Sprite, missing_sprite};
use super::quad;
use super::quad::{Affine, ScreenQuad, Surface, edge_det, normal_matrix, to_screen};
use super::rasterize::{AlphaCutout, PassKind, QuadKind};
use super::shade::shade_for;
use std::str::FromStr;
use std::sync::Arc;
use tessera_capture_gen::model;
use tessera_capture_gen::model::{Draw, DrawGeometry, Pose, QuadKind as GenQuadKind};
use ultraviolet::{Mat3, Vec3};

/// A face's corners c0 < c1 < c2 in ascending index order
const FACE_CORNERS: [[usize; 3]; 6] = {
    let mut out = [[0; 3]; 6];
    let mut d = 0;
    while d < 6 {
        let [a, b, c, e] = quad::FACE_CORNERS[d];
        let mut mask = 1u8 << a | 1 << b | 1 << c | 1 << e;
        let mut i = 0;
        while i < 3 {
            out[d][i] = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            i += 1;
        }
        d += 1;
    }
    out
};

const OUTWARD: [f32; 6] = [-1.0, 1.0, 1.0, -1.0, -1.0, 1.0];

impl From<model::Direction> for Direction {
    fn from(direction: model::Direction) -> Self {
        match direction {
            model::Direction::Down => Direction::Down,
            model::Direction::Up => Direction::Up,
            model::Direction::North => Direction::North,
            model::Direction::South => Direction::South,
            model::Direction::West => Direction::West,
            model::Direction::East => Direction::East,
        }
    }
}

pub struct CapturedMaterial {
    sprite: Arc<Sprite>,
    pass: PassKind,
    cull: bool,
    tint: u32,
    light_emission: u8,
    shade: Option<Direction>,
}

pub async fn materials(
    caches: &Caches,
    capture: Capture<'_>,
    tints: &[u32],
    subject: &ResourceId,
) -> Vec<CapturedMaterial> {
    fn cutout(value: f32, subject: &ResourceId, caches: &Caches) -> AlphaCutout {
        if value == 0.1 {
            return AlphaCutout::Tenth;
        }
        if value == 0.5 {
            return AlphaCutout::Half;
        }
        caches
            .diagnostics()
            .warn(subject, || format!("Unknown alpha cutout {value}."));
        if value < 0.3 {
            AlphaCutout::Tenth
        } else {
            AlphaCutout::Half
        }
    }

    let diag = caches.diagnostics();
    let mut out = Vec::with_capacity(capture.block.materials.len());
    for mat in capture.block.materials {
        let sprite = match ResourceId::from_str(mat.texture) {
            Ok(id) => match caches
                .textures
                .sprite(&caches.resources, &id, Quadrant::R0, (0.0, 0.0), (1.0, 1.0))
                .await
            {
                Ok(sprite) => sprite,
                Err(err) => {
                    diag.error(subject, || {
                        format!(
                            "Failed to resolve sprite for captured texture {}: {err}",
                            mat.texture
                        )
                    });
                    missing_sprite().clone()
                }
            },
            Err(_) => {
                diag.error(subject, || {
                    format!("Capture texture {} has an invalid id", mat.texture)
                });
                missing_sprite().clone()
            }
        };

        let pass = if mat.blend {
            PassKind::Translucent
        } else if let Some(value) = mat.alpha_cutout {
            PassKind::Cutout(cutout(value, subject, caches))
        } else {
            PassKind::Opaque
        };
        let tint = mat.tint_color.unwrap_or_else(|| {
            mat.tint_index
                .and_then(|idx| tints.get(usize::from(idx)).copied())
                .unwrap_or(u32::MAX)
        });

        out.push(CapturedMaterial {
            sprite,
            pass,
            cull: mat.cull,
            tint,
            light_emission: mat.light_emission,
            shade: mat.shade.map(Direction::from),
        })
    }

    out
}

pub fn project(
    cap: Capture<'_>,
    materials: &[CapturedMaterial],
    display: &Transform,
    lights: &[Vec3; 2],
    shades: &[f32; 6],
    size: u32,
    out: &mut Vec<ScreenQuad>,
) -> usize {
    fn to_affine(pose: &Pose) -> Affine {
        let mat = &pose.0;
        Affine {
            // TODO generate to col major in generator
            linear: Mat3::new(
                Vec3::new(mat[0], mat[4], mat[8]),
                Vec3::new(mat[1], mat[5], mat[9]),
                Vec3::new(mat[2], mat[6], mat[10]),
            ),
            translation: Vec3::new(mat[3], mat[7], mat[11]),
        }
    }

    fn to_surface(mat: &CapturedMaterial, shade: f32, kind: QuadKind) -> Surface {
        Surface {
            sprite: mat.sprite.clone(),
            light_emission: mat.light_emission,
            tint: mat.tint,
            shade,
            pass: mat.pass,
            kind,
        }
    }

    let size = size as f32;
    let gui = Affine::gui(display);
    let shade = |mat: &CapturedMaterial, normal: Vec3| {
        mat.shade.map_or_else(
            || shade_for(normal.normalized(), lights),
            |dir| shades[dir as usize],
        )
    };

    let mut draws: Vec<&Draw> = cap.state.draws.iter().collect();
    draws.sort_by_key(|d| d.order);

    let mut projected = 0;
    for draw in draws {
        let draw_affine = gui.after(&to_affine(draw.pose));
        match draw.geometry {
            DrawGeometry::Model { material, parts } => {
                let mat = &materials[usize::from(material)];
                let pixels = draw_affine.after(&Affine {
                    linear: Mat3::from_nonuniform_scale(Vec3::broadcast(1.0 / 16.0)),
                    translation: Vec3::zero(),
                });
                for part in parts {
                    let world = pixels.after(&to_affine(part.pose));
                    let handedness = world.linear.determinant().signum();
                    let normals = normal_matrix(world.linear);

                    for cube in part.cubes {
                        projected += 1;
                        let corners: [[f32; 3]; 8] = std::array::from_fn(|i| {
                            let pick = |bit: usize, axis: usize| {
                                if i & bit == 0 {
                                    cube.from[axis]
                                } else {
                                    cube.to[axis]
                                }
                            };
                            to_screen(world.apply(Vec3::new(pick(1, 0), pick(2, 1), pick(4, 2))), size)
                        });

                        for face in cube.faces {
                            let dir = Direction::from(face.direction);
                            let [p0, p1, p2] = FACE_CORNERS[dir as usize];
                            let quad_corners = [corners[p0], corners[p1], corners[p2]];
                            let det = edge_det(&quad_corners);

                            let front = -OUTWARD[dir as usize] * handedness * det > 0.0;
                            if det == 0.0 || (!front && mat.cull) {
                                continue;
                            }

                            let normal = normals * dir.unit();
                            let surface = to_surface(mat, shade(mat, normal), QuadKind::Parallelogram);
                            out.push(ScreenQuad::new(quad_corners, det, face.uvs, surface))
                        }
                    }
                }
            }
            DrawGeometry::BlockModel { quads } => {
                let handedness = draw_affine.linear.determinant().signum();
                let normals = normal_matrix(draw_affine.linear);
                for quad in quads {
                    projected += 1;
                    let mat = &materials[usize::from(quad.material)];
                    let [p0, p1, p2] = quad.positions.map(Vec3::from);
                    let screen = [p0, p1, p2].map(|p| to_screen(draw_affine.apply(p), size));
                    let [uv0, uv1, uv2] = quad.uvs;

                    let front = -handedness * edge_det(&screen) > 0.0;
                    let (corners, uvs, kind) = match quad.kind {
                        GenQuadKind::Parallelogram => (
                            [screen[1], screen[0], screen[2]],
                            [uv1, uv0, uv2],
                            QuadKind::Parallelogram,
                        ),
                        GenQuadKind::Triangle => (screen, quad.uvs, QuadKind::Triangle),
                    };
                    let det = edge_det(&corners);
                    if det == 0.0 || (!front && mat.cull) {
                        continue;
                    }

                    let normal = normals * (p1 - p0).cross(p2 - p0);
                    out.push(ScreenQuad::new(corners, det, uvs, to_surface(mat, shade(mat, normal), kind)))
                }
            }
        }
    }

    projected
}
