use crate::resource::ResourceId;
use crate::util::{CompactList, NonEmptyVec};
use serde::de::{Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;
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

#[derive(Deserialize)]
pub struct RawTransformation {
    pub translation: [f32; 3],
    pub left_rotation: [f32; 4],
    pub scale: [f32; 3],
    pub right_rotation: [f32; 4],
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
                formatter.write_str("an rgb value encoded in an integer or a 3-element array of floats for each color component (0.0-1.0)")
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
