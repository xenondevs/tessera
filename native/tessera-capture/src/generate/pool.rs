use super::GenerateError;
use super::states::StateGrid;
use crate::model::{Direction, Layering, QuadKind, RenderShape};
use crate::raw::{RawBlockFile, RawMaterial, RawPart, RawQuad, RawSubmission};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::path::Path;

/// Wrapper around f32's bit representation to make it hashable and comparable
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FloatingBits(u32);

impl FloatingBits {
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl From<f32> for FloatingBits {
    fn from(value: f32) -> Self {
        Self(value.to_bits())
    }
}

pub struct Pool<T> {
    items: Vec<T>,
    indices: HashMap<T, usize>,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self { items: Vec::new(), indices: HashMap::new() }
    }
}

impl<T: Clone + Eq + Hash> Pool<T> {
    pub fn pool(&mut self, item: T) -> usize {
        if let Some(&index) = self.indices.get(&item) {
            return index;
        }

        let idx = self.items.len();
        self.items.push(item.clone());
        self.indices.insert(item, idx);
        idx
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Face {
    pub direction: Direction,
    pub uvs: [[FloatingBits; 2]; 3],
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Cube {
    pub from: [FloatingBits; 3],
    pub to: [FloatingBits; 3],
    /// into `faces`
    pub faces: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Part {
    /// into `poses`
    pub pose: usize,
    /// into `cubes`
    pub cubes: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Quad {
    pub material: u8,
    pub kind: QuadKind,
    pub positions: [[FloatingBits; 3]; 3],
    pub uvs: [[FloatingBits; 2]; 3],
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum DrawGeometry {
    /// into `parts`
    Model { material: u8, parts: usize },
    /// into `quads`
    BlockModel { quads: usize },
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Draw {
    pub order: i32,
    /// into `poses`
    pub pose: usize,
    pub geometry: DrawGeometry,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Material {
    pub texture: String,
    pub pipeline: String,
    pub cull: bool,
    pub blend: bool,
    pub depth_write: bool,
    pub alpha_cutout: Option<FloatingBits>,
    /// scale factor and constant
    pub depth_bias: Option<[FloatingBits; 2]>,
    pub layering: Layering,
    pub tint_index: Option<u8>,
    pub tint_color: Option<u32>,
    pub shade: Option<Direction>,
    pub light_emission: u8,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct State {
    pub key: String,
    pub render_shape: RenderShape,
    /// into `draws`
    pub draws: usize,
}

pub struct Block {
    pub namespace: String,
    pub path: String,
    /// into `property_lists`
    pub properties: usize,
    /// into `states`
    pub states: usize,
    /// into `material_lists`
    pub materials: usize,
}

#[derive(Default)]
pub struct Pools {
    pub(super) poses: Pool<[FloatingBits; 12]>,
    pub(super) faces: Pool<Vec<Face>>,
    pub(super) cubes: Pool<Vec<Cube>>,
    pub(super) parts: Pool<Vec<Part>>,
    pub(super) quads: Pool<Vec<Quad>>,
    pub(super) materials: Pool<Material>,
    /// Runs of indices into `materials`
    pub(super) material_lists: Pool<Vec<usize>>,
    pub(super) draws: Pool<Vec<Draw>>,
    pub(super) states: Pool<Vec<State>>,
    pub(super) property_lists: Pool<Vec<String>>,
    pub(super) blocks: Vec<Block>,
}

#[derive(Copy, Clone, PartialEq)]
struct StateRecord {
    render_shape: RenderShape,
    draws: usize,
}

fn local_material(count: usize, raw: u32, path: &Path) -> Result<u8, GenerateError> {
    u8::try_from(raw)
        .ok()
        .filter(|&index| usize::from(index) < count)
        .ok_or_else(|| GenerateError::Invalid {
            path: path.to_owned(),
            message: format!("material index {raw} is out of range (0..{count})"),
        })
}

impl Pools {
    pub(super) fn block(&mut self, file: &RawBlockFile, path: &Path) -> Result<(), GenerateError> {
        let (namespace, block_path) = file.block.split_once(':').unwrap_or_else(|| ("minecraft", &file.block));
        let grid = StateGrid::parse(file, path)?;

        let materials = file
            .materials
            .iter()
            .map(|mat| self.material(mat, path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut records = Vec::with_capacity(file.states.len());
        for state in file.states.values() {
            let draws = state
                .capture
                .iter()
                .flatten()
                .map(|submission| self.draw(submission, materials.len(), path))
                .collect::<Result<Vec<_>, _>>()?;
            records.push(StateRecord {
                render_shape: state.render_shape,
                draws: self.draws.pool(draws),
            })
        }

        let relevant = grid.relevant(&records);

        // keys with identical records
        let collapsed: BTreeMap<String, StateRecord> = grid.keys(&relevant).zip(records).collect();

        let draws = &self.draws;
        let states: Vec<State> = collapsed
            .into_iter()
            // miss already means draw the pack model
            .filter(|(_, record)| record.render_shape != RenderShape::Model || !draws.items[record.draws].is_empty())
            .map(|(key, record)| State { key, render_shape: record.render_shape, draws: record.draws })
            .collect();

        if states.is_empty() {
            return Ok(());
        }

        let props = grid
            .properties
            .iter()
            .zip(&relevant)
            .filter(|(_, is_relevant)| **is_relevant)
            .map(|(prop, _)| prop.to_string())
            .collect();

        let block = Block {
            namespace: namespace.to_string(),
            path: block_path.to_string(),
            properties: self.property_lists.pool(props),
            states: self.states.pool(states),
            materials: self.material_lists.pool(materials),
        };
        self.blocks.push(block);
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<(), GenerateError> {
        self.blocks
            .sort_by(|a, b| (&a.namespace, &a.path).cmp(&(&b.namespace, &b.path)));
        let dupe = self
            .blocks
            .windows(2)
            .find(|pair| (&pair[0].namespace, &pair[0].path) == (&pair[1].namespace, &pair[1].path));
        if let Some(dupe) = dupe {
            return Err(GenerateError::DuplicateBlock { id: format!("{}:{}", dupe[0].namespace, dupe[0].path) });
        }
        Ok(())
    }

    fn material(&mut self, raw: &RawMaterial, path: &Path) -> Result<usize, GenerateError> {
        let invalid = |message: String| GenerateError::Invalid { path: path.to_owned(), message };
        let texture = match (&raw.texture.sprite, &raw.texture.path) {
            (Some(sprite), None) => sprite.clone(),
            (None, Some(path)) => {
                let (namespace, file_path) = path.split_once(':').unwrap_or(("minecraft", path));
                let id = file_path
                    .strip_prefix("textures/")
                    .and_then(|path| path.strip_suffix(".png"))
                    .ok_or_else(|| invalid(format!("invalid texture path: {path} (not in textures/**.png format)")))?;
                format!("{namespace}:{id}")
            }
            _ => {
                return Err(invalid("a texture needs either a sprite or a path, but not both".to_string()));
            }
        };
        let tint_color = match &raw.tint_color {
            None => None,
            Some(hex) => Some(
                hex.strip_prefix('#')
                    .and_then(|color| u32::from_str_radix(color, 16).ok())
                    .ok_or_else(|| invalid(format!("invalid tint color: {hex}")))?,
            ),
        };
        let tint_index = raw
            .tint_index
            .map(|idx| u8::try_from(idx).map_err(|_| invalid(format!("tint index {idx} is out of range"))))
            .transpose()?;

        Ok(self.materials.pool(Material {
            texture,
            pipeline: raw.pipeline.clone(),
            cull: raw.cull,
            blend: raw.blend,
            depth_write: raw.depth_write,
            alpha_cutout: raw.alpha_cutout.map(FloatingBits::from),
            depth_bias: raw.depth_bias.map(|bias| bias.map(FloatingBits::from)),
            layering: raw.layering,
            tint_index,
            tint_color,
            shade: raw.shade,
            light_emission: raw.light_emission,
        }))
    }

    fn pose(&mut self, pose: &[f32; 12]) -> usize {
        self.poses.pool(pose.map(FloatingBits::from))
    }

    fn draw(&mut self, submission: &RawSubmission, materials: usize, path: &Path) -> Result<Draw, GenerateError> {
        let (order, pose, geometry) = match submission {
            RawSubmission::Model { order, material, pose, parts } => (
                *order,
                pose,
                DrawGeometry::Model {
                    material: local_material(materials, *material, path)?,
                    parts: self.model_parts(parts, path)?,
                },
            ),
            RawSubmission::BlockModel { order, pose, quads } => (
                *order,
                pose,
                DrawGeometry::BlockModel { quads: self.block_model_quads(quads, materials, path)? },
            ),
        };

        Ok(Draw { order, pose: self.pose(pose), geometry })
    }

    fn model_parts(&mut self, parts: &[RawPart], path: &Path) -> Result<usize, GenerateError> {
        let invalid = |message: String| GenerateError::Invalid { path: path.to_owned(), message };
        let mut records = Vec::with_capacity(parts.len());

        for part in parts {
            let mut cubes = Vec::with_capacity(part.cubes.len());

            for cube in &part.cubes {
                let faces = cube
                    .faces
                    .iter()
                    .map(|(dir, uvs)| {
                        let [p0, p1, p2, _] = uvs.as_slice() else {
                            return Err(invalid(format!(
                                "cube face {dir} has unexpected number of UVs (expected 4, found {})",
                                uvs.len()
                            )));
                        };

                        Ok(Face {
                            direction: *dir,
                            uvs: [*p0, *p1, *p2].map(|uv| uv.map(FloatingBits::from)),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                cubes.push(Cube {
                    from: cube.from.map(FloatingBits::from),
                    to: cube.to.map(FloatingBits::from),
                    faces: self.faces.pool(faces),
                });
            }
            records.push(Part { pose: self.pose(&part.pose), cubes: self.cubes.pool(cubes) });
        }

        Ok(self.parts.pool(records))
    }

    fn block_model_quads(&mut self, quads: &[RawQuad], materials: usize, path: &Path) -> Result<usize, GenerateError> {
        let invalid = |message: String| GenerateError::Invalid { path: path.to_owned(), message };
        let mut records = Vec::with_capacity(quads.len());

        for quad in quads {
            let vertices = match quad.kind {
                QuadKind::Parallelogram => 4,
                QuadKind::Triangle => 3,
            };
            if quad.positions.len() != vertices || quad.uvs.len() != vertices {
                return Err(invalid(format!("A {:?} needs {vertices} positions and uvs)", quad.kind)));
            }

            records.push(Quad {
                material: local_material(materials, quad.material, path)?,
                kind: quad.kind,
                positions: std::array::from_fn(|i| quad.positions[i].map(FloatingBits::from)),
                uvs: std::array::from_fn(|i| quad.uvs[i].map(FloatingBits::from)),
            })
        }

        Ok(self.quads.pool(records))
    }
}
