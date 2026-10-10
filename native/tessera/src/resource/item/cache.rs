use super::raw::{RawItem, RawItemModel, RawRotation, RawSpecialModel, RawTransformation};
use super::{Item, ItemError, ItemModel};
use crate::capture::lookup;
use crate::diagnostics::Diagnostics;
use crate::resource::ResourceId;
use crate::resource::model::cache::ModelCache;
use crate::resource::resource_manager::ResourceManager;
use crate::resource::tint::TintSource;
use crate::scene::quad::Affine;
use crate::util;
use crate::util::{FastDashMap, FastHashSet, rayon_batch};
use foldhash::HashSetExt;
use rayon::iter::ParallelIterator;
use rayon::prelude::IntoParallelIterator;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;
use ultraviolet::{Mat3, Vec3};

pub struct ItemCache {
    items: FastDashMap<ResourceId, Arc<Item>>,
}

impl ItemCache {
    pub fn new() -> Self {
        Self { items: FastDashMap::default() }
    }

    pub async fn prime(&self, rm: &ResourceManager, models: &ModelCache, ids: &[ResourceId], diag: &Diagnostics) {
        let paths: Vec<String> = ids.iter().map(ResourceId::item_path).collect();
        let bytes = rm.read_many(&paths).await;
        let parsed = rayon_batch(ids.iter().cloned().zip(bytes.into_iter()).collect(), |(id, bytes)| {
            let bytes = bytes
                .map(|b| b.into_owned())
                .ok_or_else(|| ItemError::NotFound(id.clone()));
            let item = bytes.and_then(|mut b| {
                simd_json::serde::from_slice::<RawItem>(&mut b).map_err(|e| ItemError::Parse(id.clone(), e.to_string()))
            });

            (id, item)
        })
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
                model: Self::resolve(models, &id, raw.model, &Affine::identity(), diag),
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

    fn resolve(models: &ModelCache, id: &ResourceId, raw: RawItemModel, parent: &Affine, diag: &Diagnostics) -> ItemModel {
        let compose = |own: Option<&RawTransformation>| match own {
            None => *parent,
            Some(raw) => {
                let (affine, projective) = raw.to_affine();
                if projective {
                    diag.warn_keyed(id, "projective-transform", || "transformation has a projective bottom row, which was dropped.");
                }
                parent.after(&affine)
            }
        };

        let model_of = |target: &ResourceId| match models.model_cached(target, diag) {
            model @ Some(_) => model,
            None => {
                diag.error(id, || format!("item references unprimed model {target}"));
                None
            }
        };

        match raw {
            RawItemModel::Model { model, tints, transformation } => match model_of(&model) {
                Some(model) => ItemModel::Model {
                    model,
                    tints: tints.into_iter().map(TintSource::from).collect(),
                    local: compose(transformation.as_ref()),
                },
                None => ItemModel::Empty,
            },
            RawItemModel::RangeDispatch { property, scale, entries, fallback, transformation } => {
                let local = compose(transformation.as_ref());
                let mut entries: Box<[_]> = entries
                    .into_iter()
                    .map(|entry| (entry.threshold, Self::resolve(models, id, entry.model, &local, diag)))
                    .collect();
                entries.sort_by(|a, b| a.0.total_cmp(&b.0));
                ItemModel::RangeDispatch {
                    property,
                    scale,
                    entries,
                    fallback: fallback.map(|model| Box::new(Self::resolve(models, id, *model, &local, diag))),
                }
            },
            RawItemModel::Special { base, model, transformation } => {
                let capture = lookup::lookup_item(id);
                if capture.is_none() && matches!(model, RawSpecialModel::Unknown) {
                    diag.warn_keyed(id, "unsupported-special-model", || "unsupported special model type");
                }
                match model_of(&base) {
                    Some(model) => ItemModel::Special { base: model, local: compose(transformation.as_ref()), capture },
                    None => ItemModel::Empty,
                }
            }
            RawItemModel::Composite { models: children, transformation } => {
                let local = compose(transformation.as_ref());
                ItemModel::Composite(
                    children
                        .into_iter()
                        .map(|model| Self::resolve(models, id, model, &local, diag))
                        .collect(),
                )
            },
            RawItemModel::Select { property, cases, fallback, transformation } => {
                let local = compose(transformation.as_ref());
                ItemModel::Select {
                    property,
                    cases: cases
                        .into_iter()
                        .map(|case| (case.when.into_boxed(), Self::resolve(models, id, case.model, &local, diag)))
                        .collect(),
                    fallback: fallback.map(|model| Box::new(Self::resolve(models, id, *model, &local, diag))),
                }
            },
            RawItemModel::Condition { property, on_true, on_false, transformation } => {
                let local = compose(transformation.as_ref());
                ItemModel::Condition {
                    property,
                    on_true: Box::new(Self::resolve(models, id, *on_true, &local, diag)),
                    on_false: Box::new(Self::resolve(models, id, *on_false, &local, diag)),
                }
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

impl RawTransformation {
    pub fn to_affine(&self) -> (Affine, bool) {
        match self {
            RawTransformation::Decomposed { translation, left_rotation, scale, right_rotation } => {
                let linear = left_rotation.to_matrix() * Mat3::from_nonuniform_scale(Vec3::from(scale)) * right_rotation.to_matrix();
                (Affine { linear, translation: Vec3::from(translation) }, false)
            }
            RawTransformation::Matrix(mat) => {
                // row-major in, Mat3::new takes columns
                let linear = Mat3::new(
                    Vec3::new(mat[0], mat[4], mat[8]),
                    Vec3::new(mat[1], mat[5], mat[9]),
                    Vec3::new(mat[2], mat[6], mat[10])
                );
                let projective = [mat[12], mat[13], mat[14], mat[15]] != [0.0, 0.0, 0.0, 1.0];
                (Affine { linear, translation: Vec3::new(mat[3], mat[7], mat[11]) }, projective)
            }
        }
    }
}

impl RawRotation {
    //noinspection GrazieStyle
    fn to_matrix(&self) -> Mat3 {
        let [x, y, z, w] = match *self {
            RawRotation::Quaternion([x, y, z, w]) => {
                // https://github.com/JOML-CI/JOML/blob/1.10.9/src/main/java/org/joml/Quaternionf.java#L318-L325
                let inv = 1.0 / (x * x + (y * y + (z * z + w * w))).sqrt();
                [x * inv, y * inv, z * inv, w * inv]
            }
            RawRotation::AxisAngle { angle, axis: [ax, ay, az] } => {
                // normalize into [0, 2pi]
                let angle = (if angle < 0.0 { TAU + angle % TAU } else { angle }) % TAU;
                let half = angle * 0.5;

                let sin = f64::from(half).sin() as f32;
                let cos = (1.0 - sin * sin).sqrt();
                
                // cosFromSin wraps half + pi/2 into [0, 2pi] before checking the sign. half is in [0, pi],
                // so half + pi/2 is in [pi/2, 3pi/2]. so after the wrap, only the sign test is needed
                [ax * sin, ay * sin, az * sin, if half + FRAC_PI_2 >= PI { -cos } else { cos }]
            }
        };

        // Rotating v by q is the sandwich product q * v * q* (conjugate), with v as a quaternion that
        // has no w component. With u = (x, y, z) it expands to (w^2 - u*u)v + 2(u*v)u + 2w(u x v) which
        // is linear in v, so its a matrix with each column below being that expression for the basis
        // vectors (1, 0, 0), (0, 1, 0), and (0, 0, 1).
        //  - (w^2 - u*u)v keeps v's direction, so it only lands on the diagonal: w^2 - x^2 - y^2 - z^2
        //  - 2(u*v)u adds 2x^2, 2y^2 or 2z^2 to that diagonal, so one sign flips, and the symmetric off-diagonal is 2xy, 2xz, or 2yz
        //  - 2w(u x v) is perpendicular to v, so it only contributes to the off-diagonal, and is antisymmetric as +-2zw, +-2yw, +-2xw
        //
        // For a unit quaternion, q* = q^-1 and this is a pure rotation, so substitute
        // w^2 + x^2 + y^2 + z^2 = 1 and the diagonal simplifies to 1 - 2(y^2 + z^2). Nothing
        // divides by |q|^2, a non-unit q also scales by |q|^2.
        let (xx, yy, zz, ww) = (x * x, y * y, z * z, w * w);
        Mat3::new(
            Vec3::new(ww + xx - zz - yy, 2.0 * (x * y + z * w), 2.0 * (x * z - y * w)),
            Vec3::new(2.0 * (x * y - z * w), yy - zz + ww - xx, 2.0 * (y * z + x * w)),
            Vec3::new(2.0 * (x * z + y * w), 2.0 * (y * z - x * w), zz - yy - xx + ww),
        )
    }
}
