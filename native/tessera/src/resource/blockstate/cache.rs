use super::raw::{RawBlockState, RawVariants};
use super::{
    BlockStateError, BlockStateModel, ModelPart, ModelState, Pairs, PropertyRange, StateProperties,
    WeightedModels,
};
use crate::diagnostics::Diagnostics;
use crate::resource::ResourceId;
use crate::resource::model::cache::ModelCache;
use crate::resource::resource_manager::ResourceManager;
use crate::util;
use crate::util::{FastDashMap, FastHashSet, rayon_batch};
use foldhash::HashSetExt;
use rayon::iter::ParallelIterator;
use rayon::prelude::IntoParallelIterator;
use std::num::NonZeroU32;
use std::sync::Arc;

pub struct BlockStateCache {
    states: FastDashMap<ResourceId, Arc<BlockStateModel>>,
}

impl BlockStateCache {
    pub fn new() -> Self {
        Self { states: FastDashMap::default() }
    }

    pub async fn prime(
        &self,
        rm: &ResourceManager,
        models: &ModelCache,
        ids: &[ResourceId],
        diag: &Diagnostics,
    ) {
        let paths: Vec<String> = ids.iter().map(ResourceId::blockstate_path).collect();
        let bytes = rm.read_many(&paths).await;
        let parsed = rayon_batch(ids.iter().cloned().zip(bytes).collect(), |(id, bytes)| {
            let bytes = bytes
                .map(|b| b.into_owned())
                .ok_or_else(|| BlockStateError::NotFound(id.clone()));
            let state = bytes.and_then(|mut b| {
                simd_json::serde::from_slice::<RawBlockState>(&mut b)
                    .map_err(|e| BlockStateError::Parse(id.clone(), e.to_string()))
            });

            (id, state)
        })
        .await;

        let mut seen: FastHashSet<&ResourceId> = FastHashSet::new();
        for (id, result) in &parsed {
            match result {
                Ok(RawBlockState { multipart: None, variants: None }) => {
                    diag.error(id, || "Block state has no variants or multipart");
                }
                Ok(RawBlockState { multipart, variants }) => {
                    if let Some(multi) = multipart {
                        for multipart in multi.iter() {
                            Self::walk_variants(&multipart.apply, &mut seen)
                        }
                    }
                    if let Some(variants) = variants {
                        for (_, variant) in variants.iter() {
                            Self::walk_variants(variant, &mut seen)
                        }
                    }
                }
                Err(e) => {
                    diag.error(id, || format!("Failed to parse block state: {e}"));
                }
            }
        }

        models.prime(rm, seen.into_iter().cloned(), diag).await;

        let build_state = |(id, result): (ResourceId, Result<RawBlockState, BlockStateError>)| {
            let Ok(raw) = result else {
                return;
            };
            if let Some(state) = Self::build(models, &id, raw, diag) {
                self.states.insert(id, Arc::new(state));
            }
        };

        if util::is_multithreaded() {
            tokio::task::block_in_place(|| {
                parsed.into_par_iter().for_each(|pair| build_state(pair));
            });
        } else {
            parsed.into_iter().for_each(build_state);
        }
    }

    fn walk_variants<'a>(variants: &'a RawVariants, seen: &mut FastHashSet<&'a ResourceId>) {
        match variants {
            RawVariants::One(variant) => {
                seen.insert(&variant.model);
            }
            RawVariants::Weighted(variants) => {
                seen.extend(variants.iter().map(|v| &v.model));
            }
        }
    }

    fn build(
        models: &ModelCache,
        id: &ResourceId,
        raw: RawBlockState,
        diag: &Diagnostics,
    ) -> Option<BlockStateModel> {
        if matches!(raw, RawBlockState { variants: Some(_), multipart: Some(_) }) {
            diag.warn(
                id,
                || "Block state has both variants and multipart. Falling back to variants.",
            );
        }

        match raw {
            RawBlockState { variants: Some(variants), .. } => {
                let mut out = Vec::with_capacity(variants.len());
                for (key, apply) in variants {
                    if key.len() > u16::MAX as usize {
                        diag.error(id, || "variant key limit exceeds 2^16 − 1 bytes");
                        continue;
                    }
                    let weighted = Self::resolve(models, apply, diag);
                    out.push((parse_properties(key.into_boxed_str()), weighted));
                }
                (!out.is_empty()).then(|| BlockStateModel::Variants(out))
            }
            RawBlockState { multipart: Some(multipart), .. } => {
                let mut out = Vec::with_capacity(multipart.len());
                for part in multipart {
                    let weighted = Self::resolve(models, part.apply, diag);
                    out.push((part.when, weighted));
                }
                (!out.is_empty()).then(|| BlockStateModel::Multipart(out))
            }
            _ => None,
        }
    }

    fn resolve(models: &ModelCache, apply: RawVariants, diag: &Diagnostics) -> WeightedModels {
        let (one, weighted) = match apply {
            RawVariants::One(variant) => (Some(variant), Vec::new()),
            RawVariants::Weighted(variants) => (None, variants.into_vec()),
        };

        let mut out = Vec::with_capacity(one.is_some() as usize + weighted.len());
        for variant in one.into_iter().chain(weighted) {
            let weight = variant.weight.map(NonZeroU32::get).unwrap_or(1);
            let Some(model) = models.model_cached(&variant.model, diag) else {
                diag.error(&variant.model, || "Failed to resolve model");
                continue;
            };
            out.push((
                ModelPart {
                    model,
                    state: ModelState {
                        x: variant.x,
                        y: variant.y,
                        z: variant.z,
                        uv_lock: variant.uvlock,
                    },
                },
                weight,
            ));
        }

        out
    }

    pub fn get(&self, id: &ResourceId) -> Option<Arc<BlockStateModel>> {
        self.states.get(id).map(|s| s.value().clone())
    }
}

impl Default for BlockStateCache {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_properties(properties: Box<str>) -> StateProperties {
    let src = properties.as_bytes();
    let count = src.iter().filter(|&&b| b == b'=').count();

    let pairs = if count > Pairs::INLINE_PAIRS {
        let mut heap = Vec::with_capacity(count);
        scan_pairs(src, |p| heap.push(p));
        Pairs::Heap(heap.into_boxed_slice())
    } else {
        let mut buf = [PropertyRange::default(); Pairs::INLINE_PAIRS];
        let mut n = 0usize;
        scan_pairs(src, |p| {
            buf[n] = p;
            n += 1;
        });
        Pairs::Inline(buf, n as u8)
    };

    StateProperties { key: properties, pairs }
}

#[inline(always)]
fn scan_pairs(src: &[u8], mut emit: impl FnMut(PropertyRange)) {
    let mut start = 0usize;
    let mut eq = usize::MAX;
    for i in 0..=src.len() {
        if i == src.len() || src[i] == b',' {
            if eq != usize::MAX {
                emit(PropertyRange {
                    name_start: start as u16,
                    name_end: eq as u16,
                    value_start: (eq + 1) as u16,
                    value_end: i as u16,
                });
            }
            start = i + 1;
            eq = usize::MAX;
        } else if src[i] == b'=' && eq == usize::MAX {
            eq = i;
        }
    }
}
