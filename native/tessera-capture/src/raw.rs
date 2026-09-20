use crate::model::{Direction, Layering, QuadKind, RenderShape};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
pub struct RawBlockFile {
    pub format: u32,
    #[serde(rename = "minecraft")]
    pub minecraft_version: String,
    pub block: String,
    pub materials: Vec<RawMaterial>,
    pub states: BTreeMap<String, RawState>,
}

#[derive(Deserialize)]
pub struct RawMaterial {
    pub texture: RawTexture,
    pub pipeline: String,
    pub cull: bool,
    pub blend: bool,
    pub alpha_cutout: Option<f32>,
    pub depth_write: bool,
    pub depth_bias: Option<[f32; 2]>,
    pub layering: Layering,
    pub tint_index: Option<i32>,
    pub tint_color: Option<String>,
    pub shade: Option<Direction>,
    pub light_emission: u8,
}

#[derive(Deserialize)]
pub struct RawTexture {
    pub sprite: Option<String>,
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct RawState {
    pub render_shape: RenderShape,
    pub capture: Option<Vec<RawSubmission>>,
    pub discarded: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawSubmission {
    Model {
        order: i32,
        material: u32,
        pose: [f32; 12],
        parts: Vec<RawPart>,
    },
    BlockModel {
        order: i32,
        pose: [f32; 12],
        quads: Vec<RawQuad>,
    },
}

/// `path` is ignored
#[derive(Deserialize)]
pub struct RawPart {
    pub pose: [f32; 12],
    pub cubes: Vec<RawCube>,
}

#[derive(Deserialize)]
pub struct RawCube {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: BTreeMap<Direction, Vec<[f32; 2]>>,
}

#[derive(Deserialize)]
pub struct RawQuad {
    pub kind: QuadKind,
    pub material: u32,
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
}
