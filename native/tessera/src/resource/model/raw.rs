use super::{GuiLight, SlotContents};
use crate::direction::{Axis, Direction};
use crate::resource::ResourceId;
use crate::resource::texture::face::RawFace;
use crate::util::FastHashMap;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;

#[derive(Debug, Deserialize)]
pub struct RawModel {
    pub parent: Option<ResourceId>,
    #[serde(default)]
    pub textures: FastHashMap<String, SlotContents>,
    pub elements: Option<Vec<RawElement>>,
    #[serde(rename = "ambientocclusion")]
    pub ambient_occlusion: Option<bool>,
    pub gui_light: Option<GuiLight>,
    pub display: Option<RawDisplay>,
}

#[derive(Debug, Deserialize)]
pub struct RawElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    #[serde(default, deserialize_with = "deserialize_faces")]
    pub faces: Faces,
    pub rotation: Option<RawRotation>,
    pub shade_direction_override: Option<Direction>,
    #[serde(default)]
    pub light_emission: i32,
}

pub type Faces = [Option<RawFace>; 6];

pub fn deserialize_faces<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Faces, D::Error> {
    struct FacesVisitor;

    impl<'de> Visitor<'de> for FacesVisitor {
        type Value = Faces;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a map of directions to faces")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut faces = [None, None, None, None, None, None];

            while let Some((dir, face)) = map.next_entry::<Direction, RawFace>()? {
                faces[dir as usize] = Some(face);
            }

            Ok(faces)
        }
    }

    deserializer.deserialize_map(FacesVisitor)
}

#[derive(Debug, Deserialize)]
pub struct RawRotation {
    pub origin: [f32; 3],
    pub axis: Option<Axis>,
    pub angle: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    #[serde(default)]
    pub rescale: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawDisplay {
    pub thirdperson_righthand: Option<RawTransform>,
    pub thirdperson_lefthand: Option<RawTransform>,
    pub firstperson_righthand: Option<RawTransform>,
    pub firstperson_lefthand: Option<RawTransform>,
    pub head: Option<RawTransform>,
    pub gui: Option<RawTransform>,
    pub ground: Option<RawTransform>,
    pub fixed: Option<RawTransform>,
    pub on_shelf: Option<RawTransform>,
}

#[derive(Debug, Deserialize)]
pub struct RawTransform {
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default)]
    pub translation: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}
fn default_scale() -> [f32; 3] {
    [1.0; 3]
}
