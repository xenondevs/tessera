pub mod model;
pub mod pack;
pub mod texture;
pub mod resource_manager;
pub mod blockstate;
pub mod item;
pub mod tint;
pub mod cache;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
use std::fmt;
use std::fmt::Display;
use std::str::FromStr;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId {
    pub namespace: Cow<'static, str>,
    pub path: Cow<'static, str>,
}

impl ResourceId {
    pub const fn new_const(namespace: &'static str, path: &'static str) -> Self {
        Self {
            namespace: Cow::Borrowed(namespace),
            path: Cow::Borrowed(path),
        }
    }

    pub fn new<N, P>(namespace: N, path: P) -> Self
    where
        N: Into<Cow<'static, str>>,
        P: Into<Cow<'static, str>>,
    {
        Self { namespace: namespace.into(), path: path.into() }
    }

    pub fn from_path<P>(path: &P) -> Result<Self, InvalidResourceId>
    where
        P: AsRef<str>,
    {
        let raw = path.as_ref();
        let invalid = || InvalidResourceId { id: raw.to_string() };

        let rest = raw.strip_prefix("assets/").unwrap_or(raw);
        let mut parts = rest.splitn(3, '/');
        let (Some(namespace), Some(_), Some(tail)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(invalid());
        };
        let path = tail.rsplit_once(".").map_or(tail, |(path, _)| path);

        if namespace.is_empty() || path.is_empty() {
            return Err(invalid());
        }

        Ok(Self {
            namespace: match namespace {
                "minecraft" => Cow::Borrowed("minecraft"),
                namespace => Cow::Owned(namespace.to_string()),
            },
            path: Cow::Owned(path.to_string()),
        })
    }

    pub fn item_path(&self) -> String {
        format!("assets/{}/items/{}.json", self.namespace, self.path)
    }

    pub fn blockstate_path(&self) -> String {
        format!("assets/{}/blockstates/{}.json", self.namespace, self.path)
    }

    pub fn texture_path(&self) -> String {
        format!("assets/{}/textures/{}.png", self.namespace, self.path)
    }

    pub fn texture_mcmeta_path(&self) -> String {
        format!("assets/{}/textures/{}.png.mcmeta", self.namespace, self.path)
    }

    pub fn model_path(&self) -> String {
        format!("assets/{}/models/{}.json", self.namespace, self.path)
    }

    pub fn palette_path(&self) -> String {
        format!("assets/{}/textures/palettes/{}.png", self.namespace, self.path)
    }
}

#[derive(Debug, Error)]
#[error("Invalid resource id: {id}")]
pub struct InvalidResourceId {
    pub id: String,
}

impl FromStr for ResourceId {
    type Err = InvalidResourceId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[inline(always)]
        fn is_valid(part: &str, is_namespace: bool) -> bool {
            part.bytes().all(|b| match b {
                b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => true,
                b'/' if !is_namespace => true,
                _ => false,
            })
        }
        let invalid = || InvalidResourceId { id: s.to_string() };

        let (namespace, path) = s.split_once(':').unwrap_or(("", s));
        let namespace = match namespace {
            "" | "minecraft" => Cow::Borrowed("minecraft"), // saves a lot of allocs
            ".." => return Err(invalid()),
            n if is_valid(n, true) => Cow::Owned(n.to_string()),
            _ => return Err(invalid()),
        };
        if !is_valid(path, false) {
            return Err(invalid());
        }
        if path.contains("..") && path.split('/').any(|seg| seg == "..") {
            return Err(invalid());
        }
        Ok(Self { namespace, path: Cow::Owned(path.to_string()) })
    }
}

impl Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl<'de> Deserialize<'de> for ResourceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdVisitor;
        impl<'de> Visitor<'de> for IdVisitor {
            type Value = ResourceId;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a resource id like \"minecraft:block/stone\"")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<ResourceId, E> {
                ResourceId::from_str(s).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(IdVisitor)
    }
}
