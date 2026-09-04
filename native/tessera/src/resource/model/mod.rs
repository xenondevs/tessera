pub mod cache;
pub mod element;
pub mod raw;
pub mod slots;

use self::element::{Element, ShadeDirection};
use self::raw::RawModel;
use self::raw::{RawDisplay, RawTransform};
use self::slots::{SlotContents, resolve_slots};
use super::ResourceId;
use super::texture::TextureSlot;
use crate::diagnostics::Diagnostics;
use crate::util::FastHashMap;
use serde::Deserialize;
use std::sync::Arc;
use tessera_derive::EnumCount;
use thiserror::Error;
use ultraviolet::Vec3;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiLight {
    Front,
    Side,
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("No known pack provides model {0}")]
    NotFound(ResourceId),
    #[error("Failed to parse model {0}: {1}")]
    Parse(ResourceId, String),
    #[error("Element extent {from:?}..{to:?} is outside the allowed -16..=32 range")]
    ExtentOutOfBounds { from: [f32; 3], to: [f32; 3] },
    #[error("Element has no faces")]
    NoFaces,
    #[error("Invalid light emission: {0} (outside 0..=15)")]
    LightEmission(i32),
    #[error("Rotation has angle but no axis")]
    MissingAxis,
    #[error("Rotation has axis but no angle")]
    MissingAngle,
    #[error("Rotation needs either axis + angle or at least one of x, y, z")]
    MissingRotationValue,
}

#[derive(Copy, Clone, PartialEq)]
pub struct Transform {
    pub rotation: Vec3,
    pub translation: Vec3,
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        rotation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        translation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        scale: Vec3 { x: 1.0, y: 1.0, z: 1.0 },
    };

    pub const BLOCK_GUI: Self = Self {
        rotation: Vec3 { x: 30.0, y: 225.0, z: 0.0 },
        translation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        scale: Vec3 { x: 0.625, y: 0.625, z: 0.625 },
    };
}

impl From<RawTransform> for Transform {
    fn from(value: RawTransform) -> Self {
        const PIXEL_SCALE: f32 = 1.0 / 16.0;
        let translation = value.translation.map(|t| (t * PIXEL_SCALE).clamp(-5.0, 5.0));
        let scale = value.scale.map(|scale| scale.clamp(-4.0, 4.0));
        Self {
            rotation: Vec3::new(value.rotation[0], value.rotation[1], value.rotation[2]),
            translation: Vec3::from(translation),
            scale: Vec3::from(scale),
        }
    }
}

#[derive(Copy, Clone, Deserialize, EnumCount)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum DisplayContext {
    ThirdpersonLefthand,
    ThirdpersonRighthand,
    FirstpersonLefthand,
    FirstpersonRighthand,
    Head,
    Gui,
    Ground,
    Fixed,
    OnShelf,
}

pub type Display = [Option<Transform>; DisplayContext::COUNT];

impl From<RawDisplay> for Display {
    fn from(value: RawDisplay) -> Self {
        const _: () = assert!(DisplayContext::OnShelf as u8 == 8);

        let map = |raw: Option<RawTransform>| raw.map(Transform::from);

        let third_right = map(value.thirdperson_righthand);
        let first_right = map(value.firstperson_righthand);

        [
            map(value.thirdperson_lefthand).or(third_right),
            third_right,
            map(value.firstperson_lefthand).or(first_right),
            first_right,
            map(value.head),
            map(value.gui),
            map(value.ground),
            map(value.fixed),
            map(value.on_shelf),
        ]
    }
}

#[derive(Clone)]
pub enum UnresolvedGeometry {
    Cuboid(Arc<Vec<Element>>),
    GeneratedItem,
}

pub struct UnresolvedModel {
    pub parent: Option<ResourceId>,
    pub textures: FastHashMap<String, SlotContents>,
    pub geometry: Option<UnresolvedGeometry>,
    pub ambient_occlusion: Option<bool>,
    pub gui_light: Option<GuiLight>,
    pub display: Option<Box<Display>>,
}

impl TryFrom<RawModel> for UnresolvedModel {
    type Error = ModelError;

    fn try_from(value: RawModel) -> Result<Self, Self::Error> {
        let geometry = value
            .elements
            .map(|raw_elements| {
                raw_elements
                    .into_iter()
                    .map(Element::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .map(|elements| UnresolvedGeometry::Cuboid(Arc::new(elements)))
            })
            .transpose()?;

        Ok(Self {
            parent: value.parent,
            textures: value.textures,
            geometry,
            ambient_occlusion: value.ambient_occlusion,
            gui_light: value.gui_light,
            display: value.display.map(|d| Box::new(d.into())),
        })
    }
}

pub enum Geometry {
    Cuboid(Arc<Vec<Element>>),
    GeneratedItem(Arc<Vec<TextureSlot>>),
    Empty,
}

pub struct Model {
    pub geometry: Geometry,
    pub textures: FastHashMap<String, TextureSlot>,
    pub ambient_occlusion: bool,
    pub gui_light: GuiLight,
    pub display: [Option<Transform>; DisplayContext::COUNT],
}

fn collapse(chain: &[Arc<UnresolvedModel>], subject: &str, diag: &Diagnostics) -> Model {
    let slot_layers = chain.iter().map(|m| &m.textures).collect::<Vec<_>>();
    let textures = resolve_slots(&slot_layers, subject, diag);

    let geometry = match chain.iter().find_map(|m| m.geometry.clone()) {
        Some(UnresolvedGeometry::Cuboid(elements)) => Geometry::Cuboid(elements),
        Some(UnresolvedGeometry::GeneratedItem) => Geometry::GeneratedItem(item_layers(&textures)),
        None => Geometry::Empty,
    };

    let display = std::array::from_fn(|i| chain.iter().find_map(|m| m.display.as_ref()?[i]));

    if let Geometry::Cuboid(elements) = &geometry {
        let mut missing = elements
            .iter()
            .flat_map(|el| el.faces.iter().flatten())
            .map(|face| face.texture.as_str())
            .filter(|slot| !textures.contains_key(*slot))
            .collect::<Vec<_>>();

        if !missing.is_empty() {
            missing.sort_unstable();
            missing.dedup();
            diag.warn(subject, || format!("Missing texture references: {}", missing.join(", ")));
        }
    }

    Model {
        geometry,
        textures,
        ambient_occlusion: chain.iter().find_map(|m| m.ambient_occlusion).unwrap_or(true),
        gui_light: chain.iter().find_map(|m| m.gui_light).unwrap_or(GuiLight::Side),
        display,
    }
}

fn item_layers(textures: &FastHashMap<String, TextureSlot>) -> Arc<Vec<TextureSlot>> {
    // This might be infinite in the future, but currently Mojang hardcodes this as well
    // https://mcsrc.dev/1/26.3-snapshot-7/net/minecraft/client/resources/model/cuboid/ItemModelGenerator#L32
    const LAYERS: [&str; 5] = ["layer0", "layer1", "layer2", "layer3", "layer4"];
    Arc::new(LAYERS.iter().map_while(|k| textures.get(*k).cloned()).collect())
}
