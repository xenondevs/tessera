use super::Condition;
use crate::direction::Quadrant;
use crate::resource::ResourceId;
use crate::util::{NonEmptyMap, NonEmptyVec};
use serde::de::value::MapAccessDeserializer;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;
use std::num::NonZeroU32;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawBlockState {
    pub variants: Option<NonEmptyMap<String, RawVariants>>,
    pub multipart: Option<NonEmptyVec<RawMultipart>>,
}

pub enum RawVariants {
    One(RawVariant),
    Weighted(NonEmptyVec<RawVariant>),
}

impl<'de> Deserialize<'de> for RawVariants {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        struct VariantsVisitor;

        impl<'de> Visitor<'de> for VariantsVisitor {
            type Value = RawVariants;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a variant object or a non-empty array of weighted variants")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(1));
                while let Some(variant) = seq.next_element::<RawVariant>()? {
                    out.push(variant);
                }
                NonEmptyVec::new(out)
                    .map(RawVariants::Weighted)
                    .ok_or_else(|| A::Error::invalid_length(0, &"a non-empty array of weighted variants"))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                RawVariant::deserialize(MapAccessDeserializer::new(map)).map(RawVariants::One)
            }
        }

        deserializer.deserialize_any(VariantsVisitor)
    }
}

#[derive(Deserialize)]
pub struct RawVariant {
    pub model: ResourceId,
    #[serde(default)]
    pub x: Quadrant,
    #[serde(default)]
    pub y: Quadrant,
    #[serde(default)]
    pub z: Quadrant,
    #[serde(default)]
    pub uvlock: bool,
    pub weight: Option<NonZeroU32>,
}

#[derive(Deserialize)]
pub struct RawMultipart {
    pub apply: RawVariants,
    pub when: Option<Condition>,
}
