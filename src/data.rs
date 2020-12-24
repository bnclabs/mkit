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
