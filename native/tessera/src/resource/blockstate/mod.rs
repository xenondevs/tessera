mod raw;
pub mod cache;

use super::ResourceId;
use super::model::Model;
use crate::direction::Quadrant;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
use std::fmt::Formatter;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlockStateError {
    #[error("No known pack provides block state {0}")]
    NotFound(ResourceId),
    #[error("Failed to parse block state {0}: {1}")]
    Parse(ResourceId, String),
}

pub enum Condition {
    Or(Vec<Condition>),
    And(Vec<Condition>),
    /// `{"north": "side|up"}`, vec is alternatives separated by `|`
    Terms(Vec<Term>),
}

pub struct Term {
    pub property: Box<str>,
    pub alternatives: Box<str>,
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        struct ConditionVisitor;

        impl<'de> Visitor<'de> for ConditionVisitor {
            type Value = Condition;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a \"when\" clause: {\"OR\"|\"AND\": [..]} or {property: value, ..}")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let Some(first) = map.next_key::<Cow<str>>()? else {
                    return Err(A::Error::custom("Condition has no terms"));
                };

                if first == "OR" || first == "AND" {
                    let subs: Vec<Condition> = map.next_value()?;
                    if map.next_key::<IgnoredAny>()?.is_some() {
                        return Err(A::Error::custom(format!(
                            "\"{first}\" must be the only key in a combiner condition"
                        )));
                    }
                    if subs.is_empty() {
                        return Err(A::Error::custom(format!("\"{first}\" has no sub-conditions")));
                    }
                    return Ok(if first == "OR" {
                        Condition::Or(subs)
                    } else {
                        Condition::And(subs)
                    });
                }

                let mut terms = Vec::with_capacity(map.size_hint().unwrap_or(1));
                terms.push(Term {
                    property: first.into_owned().into(),
                    alternatives: map.next_value::<TermValue>()?.0,
                });

                while let Some((property, alternatives)) = map.next_entry::<Box<str>, TermValue>()? {
                    terms.push(Term { property, alternatives: alternatives.0 })
                }

                Ok(Condition::Terms(terms))
            }
        }

        deserializer.deserialize_map(ConditionVisitor)
    }
}

struct TermValue(Box<str>);

impl<'de> Deserialize<'de> for TermValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = TermValue;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a string, integer or boolean")
            }

            fn visit_bool<E: Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(TermValue(v.to_string().into()))
            }

            fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(TermValue(v.to_string().into()))
            }

            fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(TermValue(v.to_string().into()))
            }

            fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(TermValue(v.into()))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

pub enum BlockStateModel {
    Variants(Vec<(StateProperties, WeightedModels)>),
    Multipart(Vec<(Option<Condition>, WeightedModels)>),
}
enum Pairs {
    Inline([PropertyRange; Self::INLINE_PAIRS], u8),
    Heap(Box<[PropertyRange]>),
}
impl Pairs {
    const INLINE_PAIRS: usize = 8;
}

pub struct StateProperties {
    key: Box<str>,
    pairs: Pairs,
}

impl StateProperties {
    fn ranges(&self) -> &[PropertyRange] {
        match &self.pairs {
            Pairs::Inline(buf, size) => &buf[..*size as usize],
            Pairs::Heap(heap) => heap,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.ranges().iter().map(|p| {
            (
                &self.key[p.name_start as usize..p.name_end as usize],
                &self.key[p.value_start as usize..p.value_end as usize],
            )
        })
    }
}

#[derive(Copy, Clone, Default)]
struct PropertyRange {
    name_start: u16,
    name_end: u16,
    value_start: u16,
    value_end: u16,
}

pub type WeightedModels = Vec<(ModelPart, u32)>;

#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct ModelState {
    pub x: Quadrant,
    pub y: Quadrant,
    pub z: Quadrant,
    pub uv_lock: bool,
}

#[derive(Clone)]
pub struct ModelPart {
    pub model: Arc<Model>,
    pub state: ModelState,
}
