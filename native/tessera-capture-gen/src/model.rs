use std::fmt::Display;

#[derive(Debug)]
pub struct Block<'a> {
    pub namespace: &'a str,
    pub path: &'a str,
    pub properties: &'a [&'a str],
    pub states: &'a [State<'a>],
    pub materials: &'a [&'a Material<'a>]
}

#[derive(Debug)]
pub struct State<'a> {
    pub key: &'a str,
    pub render_shape: RenderShape,
    pub draws: &'a [Draw<'a>],
}

#[derive(Debug)]
pub struct Draw<'a> {
    pub order: i32,
    pub pose: &'a Pose,
    pub geometry: DrawGeometry<'a>
}

#[derive(Debug)]
pub enum DrawGeometry<'a> {
    Model { material: u8, parts: &'a [Part<'a>]},
    BlockModel { quads: &'a [Quad] }
}

#[derive(Debug)]
pub struct Pose(pub [f32; 12]);

#[derive(Debug)]
pub struct Part<'a> {
    pub pose: &'a Pose,
    pub cubes: &'a [Cube<'a>],
}

#[derive(Debug)]
pub struct Cube<'a> {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: &'a [Face],
}

#[derive(Debug)]
pub struct Face {
    pub direction: Direction,
    pub uvs: [[f32; 2]; 3],
}

#[derive(Debug)]
pub struct Quad {
    pub material: u8,
    pub kind: QuadKind,
    pub positions: [[f32; 3]; 3],
    pub uvs: [[f32; 2]; 3],
}

#[derive(Debug)]
pub struct Material<'a> {
    pub texture: &'a str,
    pub pipeline: &'a str,
    pub cull: bool,
    pub blend: bool,
    pub depth_write: bool,
    /// Texels with `alpha / 255` below this are discarded
    pub alpha_cutout: Option<f32>,
    pub depth_bias: Option<DepthBias>,
    pub layering: Layering,
    pub tint_index: Option<u8>,
    pub tint_color: Option<u32>,
    pub shade: Option<Direction>,
    pub light_emission: u8,
}

#[derive(Debug)]
pub struct DepthBias {
    pub scale: f32,
    pub constant: f32,
}

macro_rules! json_enums {
    ($($(#[$meta:meta])* pub enum $name:ident { $($variant:ident),* })*) => {
        $(
            $(#[$meta])*
            #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
            #[cfg_attr(feature = "generate", derive(serde::Deserialize), serde(rename_all = "snake_case"))]
            pub enum $name { $($variant),* }
        )*
    };
}

json_enums! {
    pub enum RenderShape { Model, Invisible }

    pub enum Layering { None, ViewOffsetZ, ViewOffsetZForward }

    /// [order ref](https://mcsrc.dev/2/26.3/net/minecraft/core/Direction#L35-40)
    pub enum Direction { Down, Up, North, South, West, East }

    pub enum QuadKind { Parallelogram, Triangle }
}

impl Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Down => write!(f, "down"),
            Direction::Up => write!(f, "up"),
            Direction::North => write!(f, "north"),
            Direction::South => write!(f, "south"),
            Direction::West => write!(f, "west"),
            Direction::East => write!(f, "east"),
        }
    }
}
