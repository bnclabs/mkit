use std::hash::Hash;

/// Trait for diffable values.
///
/// Version control is a necessary feature for non-destructive writes.
/// Using this trait it is possible to generate concise older versions
/// Note that this version control follows centralised behaviour, as
/// apposed to distributed behaviour, for which we need three-way-merge
/// trait.
///
/// If,
/// ```notest
/// P = old value; C = new value; D = difference between P and C
/// ```
///
/// Then,
/// ```notest
/// D = C - P (diff operation)
/// P = C - D (merge operation, to get old value)
/// ```
pub trait Diff: Sized + From<<Self as Diff>::D> {
    type D: Clone + From<Self> + Into<Self>;

    /// Return the delta between two consecutive versions of a value.
    /// `Delta = New - Old`.
    fn diff(&self, old: &Self) -> Self::D;

    /// Merge delta with newer version to return older version of the value.
    /// `Old = New - Delta`.
    fn merge(&self, delta: &Self::D) -> Self;
}

/// Trait to build and manage keys in a bitmapped Bloom-filter.
// TODO: should we generate 32-bit or 64-bit hashes to index into bitmap.
pub trait Bloom: Sized + Default {
    type Err: std::fmt::Display;

    /// Return the number of items in the bitmap.
    fn len(&self) -> Result<usize, Self::Err>;

    /// Add key into the index.
    fn add_key<Q: ?Sized + Hash>(&mut self, element: &Q);

    /// Add key into the index.
    fn add_digest32(&mut self, digest: u32);

    /// Check whether key in persent, there can be false positives but
    /// no false negatives.
    fn contains<Q: ?Sized + Hash>(&self, element: &Q) -> bool;

    /// Serialize the bit-map to binary array.
    fn to_vec(&self) -> Vec<u8>;

    /// Deserialize the binary array to bit-map.
    fn from_vec(buf: &[u8]) -> Result<Self, Self::Err>;

    /// Merge two bitmaps.
    fn or(&self, other: &Self) -> Result<Self, Self::Err>;
}
