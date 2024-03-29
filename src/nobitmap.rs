//! Module implement a default bitmap filter type.
use std::{convert::Infallible, hash::Hash};

use crate::{db::Bloom, Error};

/// Useful as type-parameter that implement a no-op bloom-filter.
#[derive(Clone, Default, Eq, PartialEq, Debug)]
pub struct NoBitmap;

impl Bloom for NoBitmap {
    type Err = Infallible;

    fn add_key<Q: ?Sized + Hash>(&mut self, _key: &Q) {}

    fn add_digest32(&mut self, _digest: u32) {}

    fn build(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn contains<Q: ?Sized + Hash>(&self, _element: &Q) -> bool {
        true
    }

    fn to_bytes(&self) -> Result<Vec<u8>, Self::Err> {
        Ok(vec![])
    }

    /// Deserialize the binary array to bit-map.
    fn from_bytes(_buf: &[u8]) -> Result<(Self, usize), Self::Err> {
        Ok((NoBitmap, 0))
    }

    /// Merge two bitmaps.
    fn or(&self, _other: &Self) -> Result<Self, Self::Err> {
        Ok(NoBitmap)
    }
}
