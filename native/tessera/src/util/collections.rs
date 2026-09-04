use foldhash::fast::RandomState as FastRandomState;
use serde::de::{Error, IntoDeserializer, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::Deref;

pub type FastHashMap<K, V> = std::collections::HashMap<K, V, FastRandomState>;
pub type FastHashSet<K> = std::collections::HashSet<K, FastRandomState>;
pub type FastDashMap<K, V> = dashmap::DashMap<K, V, FastRandomState>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonEmptyVec<T>(Vec<T>);

impl<T> NonEmptyVec<T> {
    pub fn new(vec: Vec<T>) -> Option<Self> {
        if vec.is_empty() { None } else { Some(Self(vec)) }
    }

    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T> Deref for NonEmptyVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> IntoIterator for NonEmptyVec<T> {
    type Item = T;
    type IntoIter = <Vec<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a NonEmptyVec<T> {
    type Item = &'a T;
    type IntoIter = <&'a Vec<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmptyVec<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let vec = Vec::<T>::deserialize(deserializer)?;
        Self::new(vec).ok_or_else(|| D::Error::custom("Expected a non-empty array"))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NonEmptyMap<K, V>(FastHashMap<K, V>);

impl<K: Eq + Hash, V> NonEmptyMap<K, V> {
    pub fn new(map: FastHashMap<K, V>) -> Option<Self> {
        if map.is_empty() { None } else { Some(Self(map)) }
    }
}

impl<K, V> Deref for NonEmptyMap<K, V> {
    type Target = FastHashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V> IntoIterator for NonEmptyMap<K, V> {
    type Item = (K, V);
    type IntoIter = <FastHashMap<K, V> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, K, V> IntoIterator for &'a NonEmptyMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = <&'a FastHashMap<K, V> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

impl<'de, K: Deserialize<'de> + Eq + Hash, V: Deserialize<'de>> Deserialize<'de> for NonEmptyMap<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let map = FastHashMap::<K, V>::deserialize(deserializer)?;
        Self::new(map).ok_or_else(|| D::Error::custom("Expected a non-empty map"))
    }
}

pub struct CompactList<T>(Box<[T]>);

impl<T> CompactList<T> {
    pub fn into_boxed(self) -> Box<[T]> {
        self.0
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for CompactList<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CompactListVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for CompactListVisitor<T> {
            type Value = CompactList<T>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a single value or a non-empty array of values")
            }

            fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
                let value = T::deserialize(v.into_deserializer())?;
                Ok(CompactList(Box::new([value])))
            }

            fn visit_borrowed_str<E: Error>(self, v: &'de str) -> Result<Self::Value, E> {
                let value = T::deserialize(v.into_deserializer())?;
                Ok(CompactList(Box::new([value])))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(1));

                while let Some(value) = seq.next_element()? {
                    out.push(value);
                }

                if out.is_empty() {
                    return Err(A::Error::invalid_length(0, &"a non-empty array"));
                }

                Ok(CompactList(out.into_boxed_slice()))
            }
        }

        deserializer.deserialize_any(CompactListVisitor(PhantomData))
    }
}
