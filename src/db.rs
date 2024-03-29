//! Module define all things database related.

use std::{borrow::Borrow, fmt, hash::Hash, ops::Bound};

#[allow(unused_imports)]
use crate::data::{Diff, NoDiff};
use crate::{Error, LocalCborize};

/// Trait to bulk-add entries into an index.
pub trait BuildIndex<K, V, D, B> {
    type Err;

    /// Build an index form iterator. Optionally a bitmap can be specified to
    /// implement a bloom filter. If bitmap filter is not required, pass bitmap
    /// as `NoBitmap`. `seqno` can be supplied to set the snapshot's seqno, if
    /// supplied as None, snapshot will take is latest-seqno as the high seqno
    /// found in the iterated entries.
    fn build_index<I>(
        &mut self,
        iter: I,
        bitmap: B,
        seqno: Option<u64>,
    ) -> Result<(), Self::Err>
    where
        I: Iterator<Item = Entry<K, V, D>>;
}

/// Trait to build and manage keys in a bit-mapped Bloom-filter.
pub trait Bloom: Sized + Default {
    type Err: fmt::Display;

    /// Add key into the index.
    fn add_key<Q: ?Sized + Hash>(&mut self, key: &Q);

    /// Add key into the index.
    fn add_digest32(&mut self, digest: u32);

    /// Build keys, added so far via `add_key` and `add_digest32` into the
    /// bitmap index. Useful for types that support batch building and
    /// immutable bitmap index.
    fn build(&mut self) -> Result<(), Error>;

    /// Check whether key in present, there can be false positives but
    /// no false negatives.
    fn contains<Q: ?Sized + Hash>(&self, element: &Q) -> bool;

    /// Serialize the bit-map to binary array.
    fn to_bytes(&self) -> Result<Vec<u8>, Self::Err>;

    /// Deserialize the binary array to bit-map.
    fn from_bytes(buf: &[u8]) -> Result<(Self, usize), Self::Err>;

    /// Merge two bitmaps.
    fn or(&self, other: &Self) -> Result<Self, Self::Err>;
}

pub trait Lookup<K, V> {
    fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: PartialEq;

    fn set(&mut self, key: K, value: V) -> Option<V>
    where
        K: PartialEq;

    fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: PartialEq;
}

const ENTRY_VER: u32 = 0x0001;
const VALUE_VER: u32 = 0x0001;
const DELTA_VER: u32 = 0x0001;

/// Value type, describe the value part of each entry withing a indexed data-set
#[derive(Clone, Debug, Eq, PartialEq, LocalCborize)]
pub enum Value<V> {
    U { value: V, seqno: u64 },
    D { seqno: u64 },
}

impl<V> Value<V> {
    pub const ID: u32 = VALUE_VER;

    pub fn set(&mut self, value: V, seqno: u64) {
        *self = Value::U { value, seqno };
    }

    pub fn delete(&mut self, seqno: u64) {
        *self = Value::D { seqno };
    }

    pub fn to_seqno(&self) -> u64 {
        match self {
            Value::U { seqno, .. } => *seqno,
            Value::D { seqno } => *seqno,
        }
    }

    pub fn is_deleted(&self) -> bool {
        match self {
            Value::U { .. } => false,
            Value::D { .. } => true,
        }
    }
}

/// Entry type, describe a single `{key,value}` entry within indexed data-set.
#[derive(Clone, Debug, Eq, PartialEq, LocalCborize)]
pub struct Entry<K, V, D = NoDiff> {
    pub key: K,
    pub value: Value<V>,
    pub deltas: Vec<Delta<D>>,
}

impl<K, V, D> Entry<K, V, D> {
    pub const ID: u32 = ENTRY_VER;
}

/// Delta type, describe the older-versions of an indexed entry.
#[derive(Clone, Debug, Eq, PartialEq, LocalCborize)]
pub enum Delta<D> {
    U { delta: D, seqno: u64 },
    D { seqno: u64 },
}

impl<D> Delta<D> {
    pub const ID: u32 = DELTA_VER;

    pub fn to_seqno(&self) -> u64 {
        match self {
            Delta::U { seqno, .. } => *seqno,
            Delta::D { seqno } => *seqno,
        }
    }
}

impl<K, V, D> From<(K, Vec<Value<V>>)> for Entry<K, V, D>
where
    V: Clone + Diff<Delta = D>,
    <V as Diff>::Delta: From<V>,
{
    fn from((key, mut values): (K, Vec<Value<V>>)) -> Self {
        if values.is_empty() {
            panic!("cannot convert empty set of values to db::Entry")
        }
        let mut entry = match values.remove(0) {
            Value::U { value, seqno } => Entry::new(key, value, seqno),
            Value::D { seqno } => Entry::new_deleted(key, seqno),
        };
        for value in values.into_iter() {
            match value {
                Value::U { value, seqno } => entry.insert(value, seqno),
                Value::D { seqno } => entry.delete(seqno),
            }
        }
        entry
    }
}

impl<K, V, D> Entry<K, V, D> {
    pub fn new(key: K, value: V, seqno: u64) -> Entry<K, V, D> {
        Entry {
            key,
            value: Value::U { value, seqno },
            deltas: Vec::default(),
        }
    }

    pub fn new_deleted(key: K, seqno: u64) -> Entry<K, V, D> {
        Entry {
            key,
            value: Value::D { seqno },
            deltas: Vec::default(),
        }
    }

    pub fn insert(&mut self, value: V, seqn: u64)
    where
        V: Clone + Diff<Delta = D>,
    {
        let delta = match self.value.clone() {
            Value::U { value: oldv, seqno } => {
                let delta: <V as Diff>::Delta = value.diff(&oldv);
                Delta::U { delta, seqno }
            }
            Value::D { seqno } => Delta::D { seqno },
        };
        self.value = Value::U { value, seqno: seqn };
        self.deltas.insert(0, delta);
    }

    pub fn delete(&mut self, seqn: u64)
    where
        V: Clone + Diff<Delta = D>,
        <V as Diff>::Delta: From<V>,
    {
        match self.value.clone() {
            Value::U { value: oldv, seqno } => {
                self.value = Value::D { seqno: seqn };

                let delta: <V as Diff>::Delta = oldv.into();
                self.deltas.insert(0, Delta::U { delta, seqno });
            }
            Value::D { seqno } => {
                self.value = Value::D { seqno: seqn };
                self.deltas.insert(0, Delta::D { seqno });
            }
        };
    }

    pub fn drain_deltas(&mut self) {
        self.deltas.drain(..);
    }
}

impl<K, V, D> Entry<K, V, D> {
    pub fn to_seqno(&self) -> u64 {
        match self.value {
            Value::U { seqno, .. } => seqno,
            Value::D { seqno } => seqno,
        }
    }

    pub fn to_key(&self) -> K
    where
        K: Clone,
    {
        self.key.clone()
    }

    pub fn to_value(&self) -> Option<V>
    where
        V: Clone,
    {
        match &self.value {
            Value::U { value, .. } => Some(value.clone()),
            Value::D { .. } => None,
        }
    }

    pub fn as_key(&self) -> &K {
        &self.key
    }

    pub fn borrow_key<Q>(&self) -> &Q
    where
        K: Borrow<Q>,
    {
        self.key.borrow()
    }

    pub fn is_deleted(&self) -> bool {
        match self.value {
            Value::U { .. } => false,
            Value::D { .. } => true,
        }
    }

    pub fn to_values(&self) -> Vec<Value<V>>
    where
        V: Diff<Delta = D> + Clone,
        D: Clone,
    {
        let mut values = vec![self.value.clone()];
        let mut val: Option<V> = self.to_value();
        for d in self.deltas.iter() {
            let (old, seqno): (Option<V>, u64) = match (val, d.clone()) {
                (Some(v), Delta::U { delta, seqno }) => (Some(v.merge(&delta)), seqno),
                (Some(_), Delta::D { seqno }) => (None, seqno),
                (None, Delta::U { delta, seqno }) => (Some(delta.into()), seqno),
                (None, Delta::D { seqno }) => (None, seqno),
            };
            values.push(
                old.clone()
                    .map(|value| Value::U { value, seqno })
                    .unwrap_or(Value::D { seqno }),
            );
            val = old;
        }

        values.reverse();

        values
    }

    pub fn contains(&self, other: &Self) -> bool
    where
        V: Clone + PartialEq + Diff<Delta = D>,
        D: Clone,
    {
        let values = self.to_values();
        other.to_values().iter().all(|v| values.contains(v))
    }

    pub fn merge(&self, other: &Self) -> Self
    where
        K: PartialEq + Clone,
        V: Clone + Diff<Delta = D>,
        D: Clone + From<V>,
    {
        if self.key != other.key {
            return self.clone();
        }

        let mut values = self.to_values();
        values.extend(other.to_values());
        values.sort_by_key(|v| v.to_seqno());

        Entry::from((self.key.clone(), values))
    }

    pub fn purge(mut self, cutoff: crate::db::Cutoff) -> Option<Self>
    where
        Self: Sized,
    {
        let (val_seqno, deleted) = match &self.value {
            Value::U { seqno, .. } => (*seqno, false),
            Value::D { seqno } => (*seqno, true),
        };

        let cutoff = match cutoff {
            crate::db::Cutoff::Mono if deleted => return None,
            crate::db::Cutoff::Mono => {
                self.deltas = vec![];
                return Some(self);
            }
            crate::db::Cutoff::Lsm(cutoff) => cutoff,
            crate::db::Cutoff::Tombstone(cutoff) if deleted => match cutoff {
                Bound::Included(cutoff) if val_seqno <= cutoff => return None,
                Bound::Excluded(cutoff) if val_seqno < cutoff => return None,
                Bound::Unbounded => return None,
                _ => return Some(self),
            },
            crate::db::Cutoff::Tombstone(_) => return Some(self),
        };

        // If all versions of this entry are before cutoff, then purge entry
        match cutoff {
            Bound::Included(std::u64::MIN) => return Some(self),
            Bound::Excluded(std::u64::MIN) => return Some(self),
            Bound::Included(cutoff) if val_seqno <= cutoff => return None,
            Bound::Excluded(cutoff) if val_seqno < cutoff => return None,
            Bound::Unbounded => return None,
            _ => (),
        }
        // Otherwise, purge only those versions that are before cutoff
        self.deltas = self
            .deltas
            .drain(..)
            .take_while(|d| {
                let seqno = match d {
                    Delta::U { seqno, .. } => *seqno,
                    Delta::D { seqno } => *seqno,
                };
                match cutoff {
                    Bound::Included(cutoff) if seqno > cutoff => true,
                    Bound::Excluded(cutoff) if seqno >= cutoff => true,
                    _ => false,
                }
            })
            .collect();
        Some(self)
    }
}

/// Cutoff is enumerated type to describe compaction behaviour.
///
/// All entries, or its versions, older than Cutoff is skipped while compaction.
/// The behavior is captured below,
///
/// _deduplication_
///
/// This is basically applicable for snapshots that don't have to preserve
/// older versions or deleted entries.
///
/// _lsm-compaction_
///
/// This is applicable for database index that store their index as multi-level
/// snapshots, similar to [leveldb][leveldb]. Most of the lsm-based-storage will
/// have their root snapshot as the oldest and only source of truth, but this
/// is not possible for distributed index that ends up with multiple truths
/// across different nodes. To facilitate such designs, in lsm mode, even the
/// root level at any given node, can retain older versions upto a specified
/// `seqno`, that `seqno` is computed through eventual consistency.
///
/// _tombstone-compaction_
///
/// Tombstone compaction is similar to `lsm-compaction` with one main
/// difference. When application logic issue `tombstone-compaction` only
/// deleted entries that are older than specified seqno will be purged.
///
/// [leveldb]: https://en.wikipedia.org/wiki/LevelDB
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cutoff {
    /// Deduplicating behavior.
    Mono,
    /// Lsm compaction.
    Lsm(Bound<u64>),
    /// Tombstone compaction.
    Tombstone(Bound<u64>),
}

#[cfg(test)]
#[path = "db_test.rs"]
mod db_test;
