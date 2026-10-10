use crate::resource::ResourceId;
use crate::util::{CompactList, NonEmptyVec};
use serde::de::{Error, Expected, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;
use serde::de::value::MapAccessDeserializer;
use tessera_derive::mc_registry_enum;

#[derive(Deserialize)]
pub struct RawItem {
    pub model: RawItemModel,
    #[serde(default = "enabled")]
    pub hand_animation_on_swap: bool,
    #[serde(default)]
    pub oversized_in_gui: bool,
    #[serde(default = "one")]
    pub swap_animation_scale: f32,
}

/// [ref](https://mcsrc.dev/1/26.3-snapshot-7/net/minecraft/client/renderer/item/ItemModels)
#[mc_registry_enum]
#[derive(Deserialize)]
pub enum RawItemModel {
    Empty,
    Model {
        model: ResourceId,
        #[serde(default)]
        tints: Vec<RawTintSource>,
        transformation: Option<RawTransformation>,
    },
    RangeDispatch {
        #[serde(flatten)]
        property: RangeProperty,
        #[serde(default = "one")]
        scale: f32,
        entries: Vec<RawRangeEntry>,
        fallback: Option<Box<RawItemModel>>,
        transformation: Option<RawTransformation>,
    },
    Special {
        base: ResourceId,
        model: RawSpecialModel,
        transformation: Option<RawTransformation>,
    },
    Composite {
        models: Vec<RawItemModel>,
        transformation: Option<RawTransformation>,
    },
    #[mc_registry(rename = "bundle/selected_item")]
    BundleSelectedItem,
    Select {
        #[serde(flatten)]
        property: SelectProperty,
        cases: NonEmptyVec<RawSelectCase>,
        fallback: Option<Box<RawItemModel>>,
        transformation: Option<RawTransformation>,
    },
    Condition {
        #[serde(flatten)]
        property: ConditionProperty,
        on_true: Box<RawItemModel>,
        on_false: Box<RawItemModel>,
        transformation: Option<RawTransformation>,
    },
    #[mc_registry(other)]
    Unknown,
}

#[mc_registry_enum]
#[derive(Deserialize)]
pub enum RawSpecialModel {
    Banner {
        color: Box<str>,
        #[serde(default = "ground")]
        attachment: Box<str>,
    },
    Bell,
    Book {
        open_angle: f32,
        page1: f32,
        page2: f32,
    },
    Chest {
        texture: ResourceId,
        #[serde(default)]
        openness: f32,
        #[serde(default)]
        chest_type: ChestType,
    },
    Conduit,
    CopperGolemStatue {
        texture: ResourceId,
        pose: Box<str>,
    },
    DecoratedPot,
    EndCube {
        effect: Box<str>,
    },
    Head {
        kind: Box<str>,
        texture: Option<ResourceId>,
        #[serde(default)]
        animation: f32,
    },
    PlayerHead,
    Shield,
    ShulkerBox {
        texture: ResourceId,
        #[serde(default)]
        openness: f32,
    },
    Trident,
    #[mc_registry(other)]
    Unknown,
}

fn ground() -> Box<str> {
    Box::from("ground")
}

#[derive(Copy, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChestType {
    #[default]
    Single,
    Left,
    Right,
}

/// [ref](https://mcsrc.dev/1/26.2/net/minecraft/client/renderer/item/properties/select/SelectItemModelProperties)
#[mc_registry_enum(tag = "property")]
#[derive(Debug, Deserialize)]
pub enum SelectProperty {
    CustomModelData {
        #[serde(default)]
        index: u32,
    },
    MainHand,
    ChargeType,
    TrimMaterial,
    BlockState {
        block_state_property: Box<str>,
    },
    DisplayContext,
    LocalTime {
        pattern: Box<str>,
        #[serde(default)]
        locale: Box<str>,
        time_zone: Option<Box<str>>,
    },
    ContextEntityType,
    ContextDimension,
    // TODO
    Component,
}

/// [ref](https://mcsrc.dev/1/26.2/net/minecraft/client/renderer/item/properties/conditional/ConditionalItemModelProperties)
#[mc_registry_enum(tag = "property")]
#[derive(Debug, Deserialize)]
pub enum ConditionProperty {
    CustomModelData {
        #[serde(default)]
        index: u32,
    },
    UsingItem,
    Broken,
    Damaged,
    #[mc_registry(rename = "fishing_rod/cast")]
    FishingRodCast,
    HasComponent {
        component: ResourceId,
        #[serde(default)]
        ignore_default: bool,
    },
    #[mc_registry(rename = "bundle/has_selected_item")]
    BundleHasSelectedItem,
    Selected,
    Carried,
    ExtendedView,
    KeybindDown {
        keybind: Box<str>,
    },
    ViewEntity,
    // TODO
    Component,
}

#[mc_registry_enum(tag = "property")]
#[derive(Debug, Deserialize)]
pub enum RangeProperty {
    CustomModelData {
        #[serde(default)]
        index: u32,
    },
    #[mc_registry(rename = "bundle/fullness")]
    BundleFullness,
    Damage {
        #[serde(default = "enabled")]
        normalize: bool,
    },
    Cooldown,
    Time {
        source: Box<str>,
        #[serde(default = "enabled")]
        wobble: bool,
    },
    Compass {
        target: Box<str>,
        #[serde(default = "enabled")]
        wobble: bool,
    },
    #[mc_registry(rename = "crossbow/pull")]
    CrossbowPull,
    UseCycle {
        #[serde(default = "one")]
        period: f32,
    },
    UseDuration {
        #[serde(default)]
        remaining: bool,
    },
    Count {
        #[serde(default = "enabled")]
        normalize: bool,
    },
}

pub enum RawTransformation {
    Decomposed {
        translation: [f32; 3],
        left_rotation: RawRotation,
        scale: [f32; 3],
        right_rotation: RawRotation,
    },
    /// row major 4x4
    Matrix([f32; 16]),
}

fn visit_fixed<'de, A: SeqAccess<'de>, const N: usize>(mut seq: A, expected: &dyn Expected) -> Result<[f32; N], A::Error> {
    let mut out = [0.0; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = seq.next_element()?.ok_or_else(|| A::Error::invalid_length(i, expected))?;
    }
    if seq.next_element::<IgnoredAny>()?.is_some() {
        return Err(A::Error::invalid_length(N + 1, expected));
    }
    Ok(out)
}

impl<'de> Deserialize<'de> for RawTransformation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Decomposed {
            translation: [f32; 3],
            left_rotation: RawRotation,
            scale: [f32; 3],
            right_rotation: RawRotation,
        }

        struct TransformationVisitor;
        impl<'de> Visitor<'de> for TransformationVisitor {
            type Value = RawTransformation;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a transformation object or a 16-element array of floats (row major 4x4 matrix)")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
                visit_fixed(seq, &self).map(RawTransformation::Matrix)
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let Decomposed { translation, left_rotation, scale, right_rotation } =
                    Decomposed::deserialize(MapAccessDeserializer::new(map))?;
                Ok(RawTransformation::Decomposed { translation, left_rotation, scale, right_rotation })
            }
        }

        deserializer.deserialize_any(TransformationVisitor)
    }
}

pub enum RawRotation {
    /// `[x, y, z, w]`
    Quaternion([f32; 4]),
    /// `angle` in radians
    AxisAngle { angle: f32, axis: [f32; 3] },
}

impl<'de> Deserialize<'de> for RawRotation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct AxisAngle {
            angle: f32,
            axis: [f32; 3],
        }

        struct RotationVisitor;
        impl<'de> Visitor<'de> for RotationVisitor {
            type Value = RawRotation;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a quaternion array [x, y, z, w] or an object with angle and axis")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
                visit_fixed(seq, &self).map(RawRotation::Quaternion)
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let AxisAngle { angle, axis } =
                    AxisAngle::deserialize(MapAccessDeserializer::new(map))?;
                Ok(RawRotation::AxisAngle { angle, axis })
            }
        }

        deserializer.deserialize_any(RotationVisitor)
    }
}

#[mc_registry_enum]
#[derive(Deserialize)]
pub enum RawTintSource {
    Constant {
        value: RawRgbValue,
    },
    Dye {
        default: RawRgbValue,
    },
    Grass {
        temperature: f32,
        downfall: f32,
    },
    Firework {
        default: RawRgbValue,
    },
    Potion {
        default: RawRgbValue,
    },
    MapColor {
        default: RawRgbValue,
    },
    Team {
        default: RawRgbValue,
    },
    #[mc_registry(other)]
    Unknown,
}

pub enum RawRgbValue {
    Integer(u32),
    FloatVector([f32; 3]),
}

impl RawRgbValue {
    pub fn into_packed_color(self) -> u32 {
        match self {
            RawRgbValue::Integer(v) => v,
            RawRgbValue::FloatVector([r, g, b]) => {
                let r = (r * 255.0) as u32;
                let g = (g * 255.0) as u32;
                let b = (b * 255.0) as u32;
                (r << 16) | (g << 8) | b
            }
        }
    }
}

impl<'de> Deserialize<'de> for RawRgbValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RgbVisitor;
        impl<'de> Visitor<'de> for RgbVisitor {
            type Value = RawRgbValue;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "an rgb value encoded in an integer or a 3-element array of floats for each color component (0.0-1.0)",
                )
            }

            fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(RawRgbValue::Integer(v as u32))
            }

            fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(RawRgbValue::Integer(v as u32))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::with_capacity(3);

                while let Some(element) = seq.next_element()? {
                    out.push(element);
                }

                if out.len() != 3 {
                    return Err(A::Error::invalid_length(out.len(), &self));
                }

                Ok(RawRgbValue::FloatVector([out[0], out[1], out[2]]))
            }
        }

        deserializer.deserialize_any(RgbVisitor)
    }
}

#[derive(Deserialize)]
pub struct RawSelectCase {
    // TODO, value codec taken from component might not be representable by a simple string
    pub when: CompactList<Box<str>>,
    pub model: RawItemModel,
}

#[derive(Deserialize)]
pub struct RawRangeEntry {
    pub threshold: f32,
    pub model: RawItemModel,
}

fn enabled() -> bool {
    true
}
fn one() -> f32 {
    1.0
}
