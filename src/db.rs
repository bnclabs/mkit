use std::{borrow::Borrow, ops::Bound};

use crate::{cbor::IntoCbor, LocalCborize};

pub trait BuildIndex<K, V, B> {
    type Err;

    fn from_iter<I, D>(self, iter: I, bitmap: B) -> Result<(), Self::Err>
    where
        I: Iterator<Item = Entry<K, V, D>>,
        D: Clone + IntoCbor;
}

const ENTRY_VER: u32 = 0x0001;
const VALUE_VER: u32 = 0x0001;
const DELTA_VER: u32 = 0x0001;
const NDIFF_VER: u32 = 0x0001;

#[derive(Clone, LocalCborize)]
pub struct NoDiff;

impl NoDiff {
    pub const ID: u32 = NDIFF_VER;
}

#[derive(Clone, LocalCborize)]
pub struct Entry<K, V, D = NoDiff> {
    pub key: K,
    pub value: Value<V>,
    pub deltas: Vec<Delta<D>>,
}

impl<K, V, D> Entry<K, V, D> {
    pub const ID: u32 = ENTRY_VER;
}

#[derive(Clone, LocalCborize)]
pub enum Value<V> {
    U { value: V, seqno: u64 },
    D { seqno: u64 },
}

impl<V> Value<V> {
    pub const ID: u32 = VALUE_VER;
}

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

/// Cutoff enumerated parameter to [compact][Index::compact] method.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cutoff {
    /// Index instances that do not need distributed LSM.
    Mono,
    /// Tombstone-compaction, refer to package-documentation for detail.
    Tombstone(Bound<u64>),
    /// Lsm-compaction, refer to package-documentation for detail.
    Lsm(Bound<u64>),
}
