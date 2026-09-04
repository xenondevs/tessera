use super::raw::{RawItem, RawItemModel};
use super::{Item, ItemError, ItemModel};
use crate::diagnostics::Diagnostics;
use crate::resource::ResourceId;
use crate::resource::model::cache::ModelCache;
use crate::resource::resource_manager::ResourceManager;
use crate::resource::tint::TintSource;
use crate::util;
use crate::util::{FastDashMap, FastHashSet, rayon_batch};
use foldhash::HashSetExt;
use rayon::iter::ParallelIterator;
use rayon::prelude::IntoParallelIterator;
use std::sync::Arc;

pub struct ItemCache {
    items: FastDashMap<ResourceId, Arc<Item>>,
}

impl ItemCache {
    pub fn new() -> Self {
        Self { items: FastDashMap::default() }
    }

    pub async fn prime(
        &self,
        rm: &ResourceManager,
        models: &ModelCache,
        ids: &[ResourceId],
        diag: &Diagnostics,
    ) {
        let paths: Vec<String> = ids.iter().map(ResourceId::item_path).collect();
        let bytes = rm.read_many(&paths).await;
        let parsed = rayon_batch(
            ids.iter().cloned().zip(bytes.into_iter()).collect(),
            |(id, bytes)| {
                let bytes = bytes
                    .map(|b| b.into_owned())
                    .ok_or_else(|| ItemError::NotFound(id.clone()));
                let item = bytes.and_then(|mut b| {
                    simd_json::serde::from_slice::<RawItem>(&mut b)
                        .map_err(|e| ItemError::Parse(id.clone(), e.to_string()))
                });

                (id, item)
            },
        )
        .await;

        let mut seen: FastHashSet<&ResourceId> = FastHashSet::new();
        for (id, result) in &parsed {
            match result {
                Ok(item) => Self::walk(&item.model, &mut seen),
                Err(e) => diag.error(id, || format!("Failed to parse item: {e}")),
            }
        }

        models.prime(rm, seen.into_iter().cloned(), diag).await;

        let build_item = |(id, result): (ResourceId, Result<RawItem, ItemError>)| {
            let Ok(raw) = result else {
                return;
            };
            let item = Item {
                model: Self::resolve(models, &id, raw.model, diag),
                oversized_in_gui: raw.oversized_in_gui,
            };
            self.items.insert(id, Arc::new(item));
        };

        if util::is_multithreaded() {
            tokio::task::block_in_place(|| {
                parsed.into_par_iter().for_each(|pair| build_item(pair));
            });
        } else {
            parsed.into_iter().for_each(build_item);
        }
    }

    fn walk<'a>(model: &'a RawItemModel, seen: &mut FastHashSet<&'a ResourceId>) {
        match model {
            RawItemModel::Model { model, .. } => {
                seen.insert(model);
            }
            RawItemModel::RangeDispatch { entries, fallback, .. } => {
                entries.iter().for_each(|entry| Self::walk(&entry.model, seen));
                if let Some(fallback) = fallback {
                    Self::walk(fallback, seen);
                }
            }
            RawItemModel::Special { base, .. } => {
                seen.insert(base);
            }
            RawItemModel::Composite { models, .. } => {
                models.iter().for_each(|model| Self::walk(model, seen));
            }
            RawItemModel::Select { cases, fallback, .. } => {
                cases.iter().for_each(|case| Self::walk(&case.model, seen));
                if let Some(fallback) = fallback {
                    Self::walk(fallback, seen);
                }
            }
            RawItemModel::Condition { on_true, on_false, .. } => {
                Self::walk(on_true, seen);
                Self::walk(on_false, seen);
            }
            RawItemModel::Empty | RawItemModel::BundleSelectedItem | RawItemModel::Unknown => {}
        }
    }

    fn resolve(models: &ModelCache, id: &ResourceId, raw: RawItemModel, diag: &Diagnostics) -> ItemModel {
        let model_of = |target: &ResourceId| match models.model_cached(target, diag) {
            model @ Some(_) => model,
            None => {
                diag.error(id, || format!("item references unprimed model {target}"));
                None
            }
        };

        match raw {
            RawItemModel::Model { model, tints, .. } => match model_of(&model) {
                Some(model) => ItemModel::Model {
                    model,
                    tints: tints.into_iter().map(TintSource::from).collect(),
                },
                None => ItemModel::Empty,
            },
            RawItemModel::RangeDispatch { property, scale, entries, fallback, .. } => {
                ItemModel::RangeDispatch {
                    property,
                    scale,
                    entries: entries
                        .into_iter()
                        .map(|entry| (entry.threshold, Self::resolve(models, id, entry.model, diag)))
                        .collect(),
                    fallback: fallback.map(|model| Box::new(Self::resolve(models, id, *model, diag))),
                }
            }
            RawItemModel::Special { base, .. } => match model_of(&base) {
                Some(model) => ItemModel::Special { base: model },
                None => ItemModel::Empty,
            },
            RawItemModel::Composite { models: children, .. } => ItemModel::Composite(
                children
                    .into_iter()
                    .map(|model| Self::resolve(models, id, model, diag))
                    .collect(),
            ),
            RawItemModel::Select { property, cases, fallback, .. } => ItemModel::Select {
                property,
                cases: cases
                    .into_iter()
                    .map(|case| {
                        (
                            case.when.into_boxed(),
                            Self::resolve(models, id, case.model, diag),
                        )
                    })
                    .collect(),
                fallback: fallback.map(|model| Box::new(Self::resolve(models, id, *model, diag))),
            },
            RawItemModel::Condition { property, on_true, on_false, .. } => ItemModel::Condition {
                property,
                on_true: Box::new(Self::resolve(models, id, *on_true, diag)),
                on_false: Box::new(Self::resolve(models, id, *on_false, diag)),
            },
            RawItemModel::Empty | RawItemModel::BundleSelectedItem => ItemModel::Empty,
            RawItemModel::Unknown => {
                diag.warn_keyed(id, "unsupported-item-model", || "unsupported item model type");
                ItemModel::Empty
            }
        }
    }

    pub fn get(&self, id: &ResourceId) -> Option<Arc<Item>> {
        self.items.get(id).map(|i| i.value().clone())
    }
}

impl Default for ItemCache {
    fn default() -> Self {
        Self::new()
    }
}
