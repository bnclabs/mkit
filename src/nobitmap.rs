use std::{convert::Infallible, hash::Hash};

use crate::db::Bloom;

/// Place holder type to skip bloom filter while building index.
#[derive(Clone, Default)]
pub struct NoBitmap;

impl Bloom for NoBitmap {
    type Err = Infallible;

    fn len(&self) -> Result<usize, Self::Err> {
        Ok(0)
    }

    fn add_key<Q: ?Sized + Hash>(&mut self, _element: &Q) {
        ()
    }

    fn add_digest32(&mut self, _digest: u32) {
        ()
    }

    fn contains<Q: ?Sized + Hash>(&self, _element: &Q) -> bool {
        true
    }

    fn to_vec(&self) -> Vec<u8> {
        vec![]
    }

    /// Deserialize the binary array to bit-map.
    fn from_vec(_buf: &[u8]) -> Result<Self, Self::Err> {
        Ok(NoBitmap)
    }

    /// Merge two bitmaps.
    fn or(&self, _other: &Self) -> Result<Self, Self::Err> {
        Ok(NoBitmap)
    }
}
