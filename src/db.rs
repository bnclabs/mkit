//! Module define all things database related.

use std::{borrow::Borrow, hash::Hash, ops::Bound};

#[allow(unused_imports)]
use crate::data::Diff;
use crate::LocalCborize;

/// Trait to bulk-add entries into an index.
pub trait BuildIndex<K, V, D, B> {
    type Err;

    /// Build an index form iterator. Optionally a bitmap can be specified to
    /// implement a bloom filter. If bitmap filter is not required, pass bitmap
    /// as `NoBitmap`.
    fn build_index<I>(&mut self, iter: I, bitmap: B) -> Result<(), Self::Err>
    where
        I: Iterator<Item = Entry<K, V, D>>;
}

/// Trait to build and manage keys in a bit-mapped Bloom-filter.
pub trait Bloom: Sized + Default {
    type Err: std::fmt::Display;

    /// Return the number of items in the bitmap.
    fn len(&self) -> Result<usize, Self::Err>;

    /// Return the number of items in the bitmap.
    fn is_empty(&self) -> Result<bool, Self::Err> {
        Ok(self.len()? == 0)
    }

    /// Add key into the index.
    fn add_key<Q: ?Sized + Hash>(&mut self, element: &Q);

    /// Add key into the index.
    fn add_digest32(&mut self, digest: u32);

    /// Check whether key in present, there can be false positives but
    /// no false negatives.
    fn contains<Q: ?Sized + Hash>(&self, element: &Q) -> bool;

    /// Serialize the bit-map to binary array.
    fn to_vec(&self) -> Vec<u8>;

    /// Deserialize the binary array to bit-map.
    fn from_vec(buf: &[u8]) -> Result<Self, Self::Err>;

    /// Merge two bitmaps.
    fn or(&self, other: &Self) -> Result<Self, Self::Err>;
}

const ENTRY_VER: u32 = 0x0001;
const VALUE_VER: u32 = 0x0001;
const DELTA_VER: u32 = 0x0001;
const NDIFF_VER: u32 = 0x0001;

/// Associated type for value-type that don't implement [Diff] trait, i.e
/// whereever applicable, use NoDiff as delta type.
#[derive(Clone, LocalCborize)]
pub struct NoDiff;

impl NoDiff {
    pub const ID: u32 = NDIFF_VER;
}

/// Entry type, describe a single `{key,value}` entry within indexed data-set.
#[derive(Clone, LocalCborize)]
pub struct Entry<K, V, D = NoDiff> {
    pub key: K,
    pub value: Value<V>,
    pub deltas: Vec<Delta<D>>,
}

impl<K, V, D> Entry<K, V, D> {
    pub const ID: u32 = ENTRY_VER;
}

/// Value type, describe the value part of each entry withing a indexed data-set
#[derive(Clone, LocalCborize)]
pub enum Value<V> {
    U { value: V, seqno: u64 },
    D { seqno: u64 },
}

impl<V> Value<V> {
    pub const ID: u32 = VALUE_VER;
}

/// Delta type, describe the older-versions of an indexed entry.
#[derive(Clone, LocalCborize)]
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

impl<K, V, D> Entry<K, V, D> {
    pub fn new(key: K, value: V) -> Entry<K, V, D> {
        Entry {
            key,
            value: Value::U { value, seqno: 0 },
            deltas: Vec::default(),
        }
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
    /// Tombstone compaction.
    Tombstone(Bound<u64>),
    /// Lsm compaction.
    Lsm(Bound<u64>),
}
