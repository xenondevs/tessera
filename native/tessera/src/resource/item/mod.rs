pub mod raw;
pub mod cache;

use self::raw::{ConditionProperty, RangeProperty, SelectProperty};
use super::ResourceId;
use super::model::Model;
use super::tint::TintSource;
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
    },
    RangeDispatch {
        property: RangeProperty,
        scale: f32,
        entries: Box<[(f32, ItemModel)]>,
        fallback: Option<Box<ItemModel>>,
    },
    Special {
        base: Arc<Model>,
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

impl ItemModel {
    pub fn gui(&self) -> &ItemModel {
        match self {
            ItemModel::Select { property, cases, fallback } => {
                if matches!(property, SelectProperty::DisplayContext) {
                    for (when, model) in cases {
                        if when.iter().any(|w| &**w == "gui") {
                            return model.gui();
                        }
                    }
                }
                fallback.as_ref().map_or(self, |model| model.gui())
            }
            ItemModel::Condition { on_false, .. } => on_false.gui(),
            ItemModel::RangeDispatch { entries, fallback, .. } => fallback
                .as_deref()
                .or_else(|| entries.first().map(|(_, model)| model))
                .map_or(self, |model| model.gui()),
            _ => self,
        }
    }
}
