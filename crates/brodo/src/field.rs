//! A field: one value per voxel of a topology's leaves, one per tile, and a background
//! (ARCHITECTURE.md §4.2, ADR-0014). It records its topology's stamp and refuses any other
//! topology as `FieldError::StaleField`.

use std::iter::repeat_n;

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::error::FieldError;
use crate::topology::{
    LeafNumber, LeafSource, Location, Part, Remap, TileNumber, TileSource, Topology,
};
use crate::tree::Coord;

/// Voxels in a leaf.
pub const LEAF_VOXELS: usize = 512;

/// A value a field holds (ARCHITECTURE.md §4.8): plain bytes, so that `freeze` can lay
/// it out and a view read it in place. Open to types outside brodo, which a field, its
/// parallel update, gathers and analyses take in memory; only a [`Stored`] type goes into a
/// frozen grid or a file (ADR-0029).
///
/// A method added to it later comes with a default, so that a type implementing it keeps
/// building. Its bounds are zerocopy 0.8's traits, which such a type derives: zerocopy is a
/// public dependency, and a release of it breaking 0.8 is taken in a major release of brodo.
pub trait Value:
    Copy + Send + Sync + 'static + FromBytes + IntoBytes + Immutable + KnownLayout
{
    /// Whether two values have the same bits.
    fn same_bits(&self, other: &Self) -> bool;
}

mod sealed {
    use super::{AnyField, Field};

    /// Keeps [`super::Stored`] to the types brodo names in its files, and wraps a field of
    /// one in its variant of [`AnyField`].
    pub trait Sealed: Sized {
        fn any(f: Field<Self>) -> AnyField;
        fn from_any(a: AnyField) -> Result<Field<Self>, AnyField>;
    }
    impl Sealed for f32 {
        fn any(f: Field<Self>) -> AnyField {
            AnyField::F32(f)
        }
        fn from_any(a: AnyField) -> Result<Field<Self>, AnyField> {
            if let AnyField::F32(f) = a { Ok(f) } else { Err(a) }
        }
    }
    impl Sealed for f64 {
        fn any(f: Field<Self>) -> AnyField {
            AnyField::F64(f)
        }
        fn from_any(a: AnyField) -> Result<Field<Self>, AnyField> {
            if let AnyField::F64(f) = a { Ok(f) } else { Err(a) }
        }
    }
    impl Sealed for u8 {
        fn any(f: Field<Self>) -> AnyField {
            AnyField::U8(f)
        }
        fn from_any(a: AnyField) -> Result<Field<Self>, AnyField> {
            if let AnyField::U8(f) = a { Ok(f) } else { Err(a) }
        }
    }
    impl Sealed for u32 {
        fn any(f: Field<Self>) -> AnyField {
            AnyField::U32(f)
        }
        fn from_any(a: AnyField) -> Result<Field<Self>, AnyField> {
            if let AnyField::U32(f) = a { Ok(f) } else { Err(a) }
        }
    }
    impl Sealed for [f32; 3] {
        fn any(f: Field<Self>) -> AnyField {
            AnyField::F32x3(f)
        }
        fn from_any(a: AnyField) -> Result<Field<Self>, AnyField> {
            if let AnyField::F32x3(f) = a { Ok(f) } else { Err(a) }
        }
    }
    impl Sealed for [f64; 3] {
        fn any(f: Field<Self>) -> AnyField {
            AnyField::F64x3(f)
        }
        fn from_any(a: AnyField) -> Result<Field<Self>, AnyField> {
            if let AnyField::F64x3(f) = a { Ok(f) } else { Err(a) }
        }
    }
}

/// A value type brodo's frozen grids and files hold, each named in them by its code
/// (docs/formats.md §1.1, ADR-0056).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValueType {
    /// `f32`, code 1.
    F32,
    /// `f64`, code 2.
    F64,
    /// `u8`, code 3.
    U8,
    /// `u32`, code 4.
    U32,
    /// `[f32; 3]`, code 5.
    F32x3,
    /// `[f64; 3]`, code 6, read by brodo from 0.3.0 on (ADR-0029).
    F64x3,
}

impl ValueType {
    /// The number naming the type in a frozen grid's header and a cache file's directory.
    pub const fn code(self) -> u32 {
        match self {
            ValueType::F32 => 1,
            ValueType::F64 => 2,
            ValueType::U8 => 3,
            ValueType::U32 => 4,
            ValueType::F32x3 => 5,
            ValueType::F64x3 => 6,
        }
    }

    /// The type a code names; none for a code no type has.
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(ValueType::F32),
            2 => Some(ValueType::F64),
            3 => Some(ValueType::U8),
            4 => Some(ValueType::U32),
            5 => Some(ValueType::F32x3),
            6 => Some(ValueType::F64x3),
            _ => None,
        }
    }

    /// The type's name, as Rust writes it: `f32`, `[f32; 3]`.
    pub const fn name(self) -> &'static str {
        match self {
            ValueType::F32 => "f32",
            ValueType::F64 => "f64",
            ValueType::U8 => "u8",
            ValueType::U32 => "u32",
            ValueType::F32x3 => "[f32; 3]",
            ValueType::F64x3 => "[f64; 3]",
        }
    }
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A field of any [`Stored`] type, the type known at run time: a grid read from a file
/// whose value type the reader did not choose (ADR-0056).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AnyField {
    /// Of `f32`.
    F32(Field<f32>),
    /// Of `f64`.
    F64(Field<f64>),
    /// Of `u8`.
    U8(Field<u8>),
    /// Of `u32`.
    U32(Field<u32>),
    /// Of `[f32; 3]`.
    F32x3(Field<[f32; 3]>),
    /// Of `[f64; 3]`.
    F64x3(Field<[f64; 3]>),
}

impl AnyField {
    /// Its values' type.
    pub fn value_type(&self) -> ValueType {
        match self {
            AnyField::F32(_) => ValueType::F32,
            AnyField::F64(_) => ValueType::F64,
            AnyField::U8(_) => ValueType::U8,
            AnyField::U32(_) => ValueType::U32,
            AnyField::F32x3(_) => ValueType::F32x3,
            AnyField::F64x3(_) => ValueType::F64x3,
        }
    }
}

impl<T: Stored> From<Field<T>> for AnyField {
    fn from(f: Field<T>) -> Self {
        T::any(f)
    }
}

/// The field, if it holds values of `T`; otherwise the `AnyField` back, unchanged.
impl<T: Stored> TryFrom<AnyField> for Field<T> {
    type Error = AnyField;

    fn try_from(a: AnyField) -> Result<Self, AnyField> {
        T::from_any(a)
    }
}

/// A value type brodo's frozen grids and files hold, named by its [`ValueType`]
/// (docs/formats.md §1.1). Sealed, so that a file holds only codes every build reads, and
/// no other type takes one of them (ADR-0029).
///
/// ```compile_fail
/// #[derive(Clone, Copy, zerocopy::FromBytes, zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::KnownLayout)]
/// #[repr(transparent)]
/// struct Count(i32);
/// impl brodo::field::Value for Count {
///     fn same_bits(&self, other: &Self) -> bool { self.0 == other.0 }
/// }
/// impl brodo::field::Stored for Count {
///     const VALUE_TYPE: brodo::field::ValueType = brodo::field::ValueType::F32;
/// }
/// ```
pub trait Stored: Value + sealed::Sealed {
    /// The type, by which its code is written.
    const VALUE_TYPE: ValueType;
}

impl Value for f32 {
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Stored for f32 {
    const VALUE_TYPE: ValueType = ValueType::F32;
}

impl Value for f64 {
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Stored for f64 {
    const VALUE_TYPE: ValueType = ValueType::F64;
}

impl Value for u8 {
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

impl Stored for u8 {
    const VALUE_TYPE: ValueType = ValueType::U8;
}

impl Value for u32 {
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

impl Stored for u32 {
    const VALUE_TYPE: ValueType = ValueType::U32;
}

impl Value for [f32; 3] {
    fn same_bits(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

impl Stored for [f32; 3] {
    const VALUE_TYPE: ValueType = ValueType::F32x3;
}

impl Value for [f64; 3] {
    fn same_bits(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

impl Stored for [f64; 3] {
    const VALUE_TYPE: ValueType = ValueType::F64x3;
}

/// Work done for one stored type, chosen at run time (`with_type`).
pub(crate) trait ForType {
    type Out;
    fn run<T: Stored>(self) -> Self::Out;
}

/// `work` run for the stored type `value_type`.
pub(crate) fn with_type<W: ForType>(value_type: ValueType, work: W) -> W::Out {
    match value_type {
        ValueType::F32 => work.run::<f32>(),
        ValueType::F64 => work.run::<f64>(),
        ValueType::U8 => work.run::<u8>(),
        ValueType::U32 => work.run::<u32>(),
        ValueType::F32x3 => work.run::<[f32; 3]>(),
        ValueType::F64x3 => work.run::<[f64; 3]>(),
    }
}

/// How a merged tile's value comes from the values it covers, each with the number of
/// voxels holding it (ADR-0014); `None` refuses the merge.
pub type MergeRule<T> = dyn Fn(&[(T, u32)]) -> Option<T> + Send + Sync;

/// The merge rule that accepts only values with the same bits.
pub fn uniform<T: Value>(values: &[(T, u32)]) -> Option<T> {
    let (first, _) = *values.first()?;
    values.iter().all(|(v, _)| v.same_bits(&first)).then_some(first)
}

/// Voxels in a leaf, as a count.
const LEAF_COUNT: u32 = 512;

/// A voxel's place within its leaf, x fastest: below 512.
#[inline]
pub(crate) fn leaf_place(v: Coord) -> u32 {
    let [x, y, z] = v.map(|c| c.cast_unsigned() & 7);
    x + 8 * y + 64 * z
}

/// A voxel's index within its leaf, x fastest.
#[inline]
pub(crate) fn local_index(v: Coord) -> usize {
    // Lossless: below 512.
    leaf_place(v) as usize
}

/// One value per voxel of a topology's leaves, one per tile, and a background.
#[derive(Debug, Clone)]
pub struct Field<T> {
    /// Leaf count × 512, by leaf number.
    leaf_values: Vec<T>,
    /// One per tile, by tile number.
    tile_values: Vec<T>,
    background: T,
    stamp: u64,
}

impl<T: Value> Field<T> {
    /// A field on `t` with every value the background.
    pub fn new(t: &Topology, background: T) -> Self {
        Self {
            leaf_values: vec![background; t.leaf_count() * LEAF_VOXELS],
            tile_values: vec![background; t.tile_count()],
            background,
            stamp: t.stamp(),
        }
    }

    /// The value outside the tree.
    pub fn background(&self) -> T {
        self.background
    }

    /// Sets the value outside the tree. The leaves' and tiles' values are unchanged, those
    /// equal to the old background among them.
    pub fn set_background(&mut self, value: T) {
        self.background = value;
    }

    pub(crate) fn check(&self, t: &Topology) -> Result<(), FieldError> {
        if self.stamp == t.stamp() { Ok(()) } else { Err(FieldError::StaleField) }
    }

    /// Every leaf's values, by leaf number, if the field belongs to `t`.
    pub(crate) fn leaves(&self, t: &Topology) -> Result<&[[T; LEAF_VOXELS]], FieldError> {
        self.check(t)?;
        Ok(self.leaf_values.as_chunks().0)
    }

    /// Every leaf's values to write, by leaf number, if the field belongs to `t`.
    pub(crate) fn leaves_mut(
        &mut self,
        t: &Topology,
    ) -> Result<&mut [[T; LEAF_VOXELS]], FieldError> {
        self.check(t)?;
        Ok(self.leaf_values.as_chunks_mut().0)
    }

    fn leaf_slice(&self, n: LeafNumber) -> Result<&[T], FieldError> {
        let s = n.get() as usize * LEAF_VOXELS;
        self.leaf_values.get(s..s + LEAF_VOXELS).ok_or(FieldError::UnknownLeaf(n.get()))
    }

    fn tile_value(&self, n: TileNumber) -> Result<T, FieldError> {
        self.tile_values.get(n.get() as usize).copied().ok_or(FieldError::UnknownTile(n.get()))
    }

    /// A leaf's 512 values, x fastest.
    pub fn leaf(&self, t: &Topology, n: LeafNumber) -> Result<&[T; LEAF_VOXELS], FieldError> {
        self.check(t)?;
        self.leaf_slice(n)?.try_into().map_err(|_| FieldError::UnknownLeaf(n.get()))
    }

    /// A leaf's 512 values, to write.
    pub fn leaf_mut(
        &mut self,
        t: &Topology,
        n: LeafNumber,
    ) -> Result<&mut [T; LEAF_VOXELS], FieldError> {
        self.check(t)?;
        let s = n.get() as usize * LEAF_VOXELS;
        let slice = self.leaf_values.get_mut(s..s + LEAF_VOXELS);
        slice.and_then(|c| c.try_into().ok()).ok_or(FieldError::UnknownLeaf(n.get()))
    }

    /// A tile's value.
    pub fn tile(&self, t: &Topology, n: TileNumber) -> Result<T, FieldError> {
        self.check(t)?;
        self.tile_value(n)
    }

    /// Sets a tile's value.
    pub fn set_tile(&mut self, t: &Topology, n: TileNumber, value: T) -> Result<(), FieldError> {
        self.check(t)?;
        let slot = self.tile_values.get_mut(n.get() as usize);
        *slot.ok_or(FieldError::UnknownTile(n.get()))? = value;
        Ok(())
    }

    /// The value at a voxel: its leaf's, its tile's, or the background.
    pub fn value_at(&self, t: &Topology, v: Coord) -> Result<T, FieldError> {
        self.check(t)?;
        Ok(self.at(t.locate(v), v))
    }

    /// The value at a voxel, given what holds it in this field's topology.
    pub(crate) fn at(&self, loc: Location, v: Coord) -> T {
        match loc {
            // Numbers from this field's topology, checked by its stamp, index its values.
            Location::Leaf(n) => self.leaf_values[n.get() as usize * LEAF_VOXELS + local_index(v)],
            Location::Tile(n, _) => self.tile_values[n.get() as usize],
            Location::Background => self.background,
        }
    }

    /// The value at a voxel of leaf `n` of this field's topology, to write.
    pub(crate) fn at_mut(&mut self, n: LeafNumber, v: Coord) -> &mut T {
        &mut self.leaf_values[n.get() as usize * LEAF_VOXELS + local_index(v)]
    }

    /// A tile's value by its topology's tile number.
    pub(crate) fn tile_value_at(&self, n: u32) -> Result<T, FieldError> {
        self.tile_values.get(n as usize).copied().ok_or(FieldError::UnknownTile(n))
    }

    /// Whether the field belongs to `t`.
    pub(crate) fn belongs_to(&self, t: &Topology) -> Result<(), FieldError> {
        self.check(t)
    }

    /// The values a merged tile covers, each with its voxel count.
    fn merged(&self, parts: &[Part]) -> Result<Vec<(T, u32)>, FieldError> {
        let mut out = Vec::new();
        for p in parts {
            match *p {
                Part::Leaf(n) => out.extend(self.leaf_slice(n)?.iter().map(|v| (*v, 1))),
                Part::Tile(t) => out.push((self.tile_value(t)?, LEAF_COUNT)),
                Part::Background(k) => out.push((self.background, LEAF_COUNT.saturating_mul(k))),
            }
        }
        Ok(out)
    }

    /// This field on the remap's new topology: each new leaf from its old leaf, from the
    /// tile it was split from, or the background; each new tile from its old tile, the
    /// background, or `rule` applied to what it merges. Merged tiles are worked out in tile
    /// order, so the error does not depend on the thread count.
    pub fn remap(&self, r: &Remap, rule: &MergeRule<T>) -> Result<Field<T>, FieldError> {
        if self.stamp != r.from {
            return Err(FieldError::StaleField);
        }
        let mut leaf_values = Vec::with_capacity(r.leaf_sources().len() * LEAF_VOXELS);
        for s in r.leaf_sources() {
            match *s {
                LeafSource::Leaf(n) => leaf_values.extend_from_slice(self.leaf_slice(n)?),
                LeafSource::Split(t) => {
                    leaf_values.extend(repeat_n(self.tile_value(t)?, LEAF_VOXELS))
                }
                LeafSource::Background => {
                    leaf_values.extend(repeat_n(self.background, LEAF_VOXELS))
                }
            }
        }
        let tile_values = r
            .tile_sources()
            .iter()
            .zip(0_u32..)
            .map(|(s, i)| match *s {
                TileSource::Tile(t) => self.tile_value(t),
                TileSource::Background => Ok(self.background),
                TileSource::Merged(range) => {
                    rule(&self.merged(r.parts(range))?).ok_or(FieldError::MergeRefused(i))
                }
            })
            .collect::<Result<Vec<T>, FieldError>>()?;
        Ok(Field { leaf_values, tile_values, background: self.background, stamp: r.to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TileLevel;

    /// Each value type is the code and name docs/formats.md §1.1 gives it, and a field of
    /// it the variant of `AnyField` of its name.
    #[test]
    fn each_value_type_is_its_code_and_name_and_its_field_its_variant() {
        let t = Topology::new();
        let fields: [(AnyField, ValueType, u32, &str); 6] = [
            (Field::new(&t, 0.0_f32).into(), f32::VALUE_TYPE, 1, "f32"),
            (Field::new(&t, 0.0_f64).into(), f64::VALUE_TYPE, 2, "f64"),
            (Field::new(&t, 0_u8).into(), u8::VALUE_TYPE, 3, "u8"),
            (Field::new(&t, 0_u32).into(), u32::VALUE_TYPE, 4, "u32"),
            (Field::new(&t, [0.0_f32; 3]).into(), <[f32; 3]>::VALUE_TYPE, 5, "[f32; 3]"),
            (Field::new(&t, [0.0_f64; 3]).into(), <[f64; 3]>::VALUE_TYPE, 6, "[f64; 3]"),
        ];
        for (field, ty, code, name) in fields {
            assert_eq!(field.value_type(), ty);
            assert_eq!((ty.code(), ty.name(), ty.to_string()), (code, name, name.to_owned()));
            assert_eq!(ValueType::from_code(code), Some(ty));
        }
        assert_eq!([0, 7, u32::MAX].map(ValueType::from_code), [None; 3]);
    }

    /// A field is taken back from an `AnyField` holding its type, and an `AnyField` of
    /// another type is given back unchanged.
    #[test]
    fn a_field_is_taken_from_an_any_field_of_its_type_only() {
        let t = Topology::new();
        let mut f = Field::new(&t, 2.5_f32);
        f.set_background(-1.0);
        let back = Field::<f32>::try_from(AnyField::from(f)).unwrap();
        assert_eq!(back.background().to_bits(), (-1.0_f32).to_bits());
        let refused = Field::<f64>::try_from(AnyField::from(back)).unwrap_err();
        assert_eq!(refused.value_type(), ValueType::F32);
        assert!(Field::<[f32; 3]>::try_from(AnyField::from(Field::new(&t, 0_u8))).is_err());
        let wide = AnyField::from(Field::new(&t, [1.0_f64; 3]));
        assert_eq!(Field::<[f64; 3]>::try_from(wide).ok().map(|f| f.background()), Some([1.0; 3]));
    }

    /// Every value type compares by bits: equal values agree, a zero's sign and a NaN's
    /// payload are told apart, and so is any other difference, in each component.
    #[test]
    fn every_value_type_compares_by_bits() {
        assert!(1.5_f32.same_bits(&1.5) && !0.0_f32.same_bits(&-0.0) && !1.0_f32.same_bits(&2.0));
        assert!(1.5_f64.same_bits(&1.5) && !0.0_f64.same_bits(&-0.0) && !1.0_f64.same_bits(&2.0));
        assert!(f64::NAN.same_bits(&f64::NAN) && !f64::NAN.same_bits(&-f64::NAN));
        assert!(7_u8.same_bits(&7) && !7_u8.same_bits(&8));
        assert!(7_u32.same_bits(&7) && !7_u32.same_bits(&8));
        assert!([1.0_f32, 2.0, 3.0].same_bits(&[1.0, 2.0, 3.0]));
        assert!(![1.0_f32, 2.0, 3.0].same_bits(&[1.0, 2.0, -3.0]));
        assert!([1.0_f64, 2.0, 3.0].same_bits(&[1.0, 2.0, 3.0]));
        assert!(![1.0_f64, 2.0, 3.0].same_bits(&[-1.0, 2.0, 3.0]));
    }

    /// A new background is read outside the tree, and the leaves' and tiles' values,
    /// those equal to the old background among them, are unchanged.
    #[test]
    fn a_new_background_leaves_the_leaves_and_tiles_as_they_were() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        e.activate([0, 0, 0]).unwrap();
        e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, 0.0_f32);
        f.leaf_mut(&t, LeafNumber(0)).unwrap()[1] = 3.0;
        f.set_background(-7.0);
        let at = |v| f.value_at(&t, v).map(f32::to_bits);
        assert_eq!(f.background().to_bits(), (-7.0_f32).to_bits());
        assert_eq!(at([-1, 0, 0]), Ok((-7.0_f32).to_bits()));
        assert_eq!(at([0, 0, 0]), Ok(0.0_f32.to_bits()));
        assert_eq!(at([1, 0, 0]), Ok(3.0_f32.to_bits()));
        assert_eq!(at([9, 0, 0]), Ok(0.0_f32.to_bits()));
        let leaf = LeafNumber::new(0);
        assert_eq!(f.leaf(&t, leaf).map(|l| l[1].to_bits()), Ok(3.0_f32.to_bits()));
        assert_eq!(f.tile(&t, TileNumber::new(0)).map(f32::to_bits), Ok(0.0_f32.to_bits()));
        assert_eq!(f.leaf(&t, LeafNumber::new(1)).map(|_| ()), Err(FieldError::UnknownLeaf(1)));
        assert_eq!(f.tile(&t, TileNumber::new(1)), Err(FieldError::UnknownTile(1)));
    }

    #[test]
    fn a_root_tile_split_passes_its_value_to_every_part() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        e.add_tile(TileLevel::Root, [0, 0, 0]).unwrap();
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 0.0_f32);
        f.set_tile(&t1, TileNumber(0), 5.0).unwrap();
        let mut e = t1.edit();
        e.split_tile([300, 4000, 17]).unwrap();
        let (t2, r) = e.commit().unwrap();
        let g = f.remap(&r, &uniform).unwrap();
        assert_eq!(t2.tile_count(), 32767 + 4095);
        for n in t2.tile_numbers() {
            assert_eq!(g.tile(&t2, n).unwrap().to_bits(), 5.0_f32.to_bits());
        }
        let leaf = g.leaf(&t2, LeafNumber(0)).unwrap();
        assert!(leaf.iter().all(|v| v.to_bits() == 5.0_f32.to_bits()));
        assert_eq!(g.value_at(&t2, [-1, 0, 0]).map(f32::to_bits), Ok(0.0_f32.to_bits()));
    }

    /// The voxel-weighted mean, exact here: values and counts are small integers.
    fn mean(values: &[(f64, u32)]) -> Option<f64> {
        let n: f64 = values.iter().map(|(_, c)| f64::from(*c)).sum();
        Some(values.iter().map(|(v, c)| v * f64::from(*c)).sum::<f64>() / n)
    }

    #[test]
    fn a_merge_weighs_each_value_by_its_voxels() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        for i in 0..4096 {
            let r = [i % 16 * 8, i / 16 % 16 * 8, i / 256 * 8];
            if i == 0 { e.activate(r).map(|_| ()) } else { e.add_tile(TileLevel::Lower, r) }
                .unwrap();
        }
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 0.0_f64);
        for n in t1.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t1, n, 2.0).unwrap();
        }
        let leaf = t1.leaf_numbers().next().unwrap();
        f.leaf_mut(&t1, leaf).unwrap().fill(4.0);
        let mut e = t1.edit();
        e.merge(TileLevel::Upper, [0, 0, 0]).unwrap();
        let (t2, r) = e.commit().unwrap();
        let g = f.remap(&r, &mean).unwrap();
        // 4095 tiles of 512 voxels at 2 and one leaf of 512 at 4: 2 + 2 / 4096. Weighing
        // each tile as one value would give 10 238 / 4607 instead.
        let n = t2.tile_numbers().next().unwrap();
        assert_eq!(g.tile(&t2, n).map(f64::to_bits), Ok((2.0 + 2.0 / 4096.0_f64).to_bits()));
        let refused = f.remap(&r, &uniform);
        assert_eq!(refused.map(|_| ()), Err(FieldError::MergeRefused(0)));
    }

    #[test]
    fn regions_activated_in_the_merging_commit_count_as_the_background() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        for i in 0..2048 {
            e.add_tile(TileLevel::Lower, [i % 16 * 8, i / 16 % 16 * 8, i / 256 * 8]).unwrap();
        }
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 3.0_f32);
        let mut e = t1.edit();
        for i in 2048..4096 {
            e.activate([i % 16 * 8, i / 16 % 16 * 8, i / 256 * 8]).unwrap();
        }
        e.merge(TileLevel::Upper, [0, 0, 0]).unwrap();
        let (t2, r) = e.commit().unwrap();
        let TileSource::Merged(range) = r.tile_sources()[0] else { panic!("merged") };
        assert_eq!(r.parts(range).last(), Some(&Part::Background(2048)));
        let g = f.remap(&r, &uniform).unwrap();
        assert_eq!(g.value_at(&t2, [100, 100, 100]).map(f32::to_bits), Ok(3.0_f32.to_bits()));
        let n = t1.tile_numbers().next().unwrap();
        f.set_tile(&t1, n, 1.0).unwrap();
        assert_eq!(f.remap(&r, &uniform).map(|_| ()), Err(FieldError::MergeRefused(0)));
    }

    #[test]
    fn a_stale_field_is_refused_everywhere() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        e.activate([0, 0, 0]).unwrap();
        e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 1_u8);
        let (t2, r) = t1.edit().commit().unwrap();
        let stale = Err(FieldError::StaleField);
        assert_eq!(f.leaf(&t2, LeafNumber(0)).map(|_| ()), stale);
        assert_eq!(f.tile(&t2, TileNumber(0)).map(|_| ()), stale);
        assert_eq!(f.value_at(&t2, [0, 0, 0]).map(|_| ()), stale);
        assert_eq!(f.set_tile(&t2, TileNumber(0), 2), stale);
        assert_eq!(f.leaf_mut(&t2, LeafNumber(0)).map(|_| ()), stale);
        let g = f.remap(&r, &uniform).unwrap();
        assert_eq!(g.remap(&r, &uniform).map(|_| ()), stale, "a remap applies once");
        assert_eq!(g.value_at(&t2, [0, 0, 0]), Ok(1));
        assert_eq!(f.value_at(&t1, [0, 0, 0]), Ok(1), "the old pair still works");
    }

    #[test]
    fn a_voxel_finds_its_place_in_its_leaf() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        e.activate([-1, 2, 3]).unwrap();
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, 0_u32);
        for (i, v) in f.leaf_mut(&t, LeafNumber(0)).unwrap().iter_mut().enumerate() {
            *v = u32::try_from(i).unwrap();
        }
        // [-1, 2, 3] is at (7, 2, 3) within the leaf whose origin is [-8, 0, 0].
        assert_eq!(f.value_at(&t, [-1, 2, 3]), Ok(7 + 8 * 2 + 64 * 3));
        assert_eq!(f.leaf(&t, LeafNumber(1)).map(|_| ()), Err(FieldError::UnknownLeaf(1)));
    }
}
