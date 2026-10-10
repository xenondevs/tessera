pub mod raw;
pub mod cache;

use std::borrow::Cow;
use self::raw::{ConditionProperty, RangeProperty, SelectProperty};
use super::ResourceId;
use super::model::Model;
use super::tint::TintSource;
use crate::capture::lookup::Capture;
use crate::scene::quad::Affine;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ItemError {
    #[error("No known pack provides item {0}")]
    NotFound(ResourceId),
    #[error("Failed to parse item {0}: {1}")]
    Parse(ResourceId, String),
}

pub struct Item {
    pub model: ItemModel,
    pub oversized_in_gui: bool,
}

pub enum ItemModel {
    Empty,
    Model {
        model: Arc<Model>,
        tints: Box<[TintSource]>,
        local: Affine,
    },
    RangeDispatch {
        property: RangeProperty,
        scale: f32,
        entries: Box<[(f32, ItemModel)]>,
        fallback: Option<Box<ItemModel>>,
    },
    Special {
        base: Arc<Model>,
        local: Affine,
        capture: Option<Capture<'static>>,
    },
    Composite(Box<[ItemModel]>),
    Select {
        property: SelectProperty,
        cases: Box<[(Box<[Box<str>]>, ItemModel)]>,
        fallback: Option<Box<ItemModel>>,
    },
    Condition {
        property: ConditionProperty,
        on_true: Box<ItemModel>,
        on_false: Box<ItemModel>,
    },
}

pub enum Leaf<'a> {
    Model {
        model: &'a Arc<Model>,
        tints: &'a [TintSource],
        local: &'a Affine,
    },
    Special {
        base: &'a Arc<Model>,
        local: &'a Affine,
        capture: Option<Capture<'static>>,
    },
    Missing,
}

impl ItemModel {
    pub fn leaves<'a, CTX: ItemContext>(&'a self, ctx: &CTX, out: &mut Vec<Leaf<'a>>) {
        fn pick<'a, CTX: ItemContext>(model: Option<&'a ItemModel>, ctx: &CTX, out: &mut Vec<Leaf<'a>>) {
            match model {
                Some(model) => model.leaves(ctx, out),
                None => out.push(Leaf::Missing),
            }
        }

        match self {
            ItemModel::Empty => {}
            ItemModel::Model { model, tints, local } => out.push(Leaf::Model { model, tints, local }),
            ItemModel::RangeDispatch { property, scale, entries, fallback } => {
                let value = ctx.range(property) * scale;
                let entry = match entries.partition_point(|(threshold, _)| *threshold <= value) {
                    0 => None,
                    idx => Some(&entries[idx - 1].1),
                };
                pick(entry.or(fallback.as_deref()), ctx, out)
            }
            ItemModel::Special { base, local, capture } => out.push(Leaf::Special { base, local, capture: *capture }),
            ItemModel::Composite(children) => children.iter().for_each(|child| child.leaves(ctx, out)),
            ItemModel::Select { property, cases, fallback } => {
                let case = ctx.select(property).and_then(|val| {
                    cases.iter().find(|(when, _)| when.iter().any(|w| &**w == val))
                });
                pick(case.map(|(_, model)| model).or(fallback.as_deref()), ctx, out)
            }
            ItemModel::Condition { property, on_true, on_false } => {
                (if ctx.condition(property) { on_true } else { on_false }).leaves(ctx, out)
            }
        }
    }
}

pub trait ItemContext {
    fn select(&self, property: &SelectProperty) -> Option<Cow<'static, str>>;
    fn condition(&self, property: &ConditionProperty) -> bool;
    fn range(&self, property: &RangeProperty) -> f32;
}

pub struct StaticContext;

impl ItemContext for StaticContext {
    fn select(&self, property: &SelectProperty) -> Option<Cow<'static, str>> {
        match property {
            SelectProperty::ChargeType => Some(Cow::Borrowed("none")),
            SelectProperty::DisplayContext => Some(Cow::Borrowed("gui")),
            _ => None,
        }
    }

    // TODO has_component for an item's default components
    fn condition(&self, _: &ConditionProperty) -> bool {
        false
    }

    fn range(&self, property: &RangeProperty) -> f32 {
        match property {
            RangeProperty::Count { normalize: true } => 1.0 / 64.0,
            RangeProperty::Count { normalize: false } => 1.0,
            _ => 0.0,
        }
    }
}
