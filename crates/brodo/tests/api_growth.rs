//! P4's acceptance (ADR-0035), brodo's half: code written as a user outside the crate
//! writes it, against every type that may grow. Each `match` names every variant there is
//! and ends in a wildcard arm: a `#[non_exhaustive]` enum requires that arm, and an enum
//! without the attribute makes it unreachable, which the gate's lints deny, so a type
//! losing the attribute fails the gate. Each struct a user builds is built by its
//! constructor and set by assignment, which a field added later leaves working.

use brodo::analysis::AnalysisError;
use brodo::cache::CacheFile;
use brodo::error::{
    CacheError, FieldError, FormatError, FreezeError, HermiteError, MetaError, Rule, SliceError,
    TreeError, VtkError,
};
use brodo::gather::GhostWidth;
use brodo::grid::GridError;
use brodo::meta::GridMeta;
#[cfg(feature = "slice-png")]
use brodo::slice::{Axis, ColorMap, Image, Slice};

fn analysis_error(e: &AnalysisError) -> &'static str {
    match e {
        AnalysisError::Field(..) => "Field",
        AnalysisError::Tree(..) => "Tree",
        AnalysisError::TooSpread => "TooSpread",
        AnalysisError::TooLarge => "TooLarge",
        AnalysisError::Unlabelled(..) => "Unlabelled",
        AnalysisError::Unbounded => "Unbounded",
        _ => "later",
    }
}

fn tree_error(e: &TreeError) -> &'static str {
    match e {
        TreeError::NotATile(..) => "NotATile",
        TreeError::InTile { .. } => "InTile",
        TreeError::Occupied { .. } => "Occupied",
        TreeError::ArenaFull(..) => "ArenaFull",
        TreeError::UnknownLeaf(..) => "UnknownLeaf",
        TreeError::UnknownTile(..) => "UnknownTile",
        TreeError::TooManyTiles => "TooManyTiles",
        TreeError::TooManyParts => "TooManyParts",
        TreeError::NotMergeable { .. } => "NotMergeable",
        TreeError::HoldsEmpty { .. } => "HoldsEmpty",
        TreeError::MergeUnsupported(..) => "MergeUnsupported",
        _ => "later",
    }
}

fn field_error(e: &FieldError) -> &'static str {
    match e {
        FieldError::StaleField => "StaleField",
        FieldError::UnknownLeaf(..) => "UnknownLeaf",
        FieldError::UnknownTile(..) => "UnknownTile",
        FieldError::MergeRefused(..) => "MergeRefused",
        FieldError::NotALeaf { .. } => "NotALeaf",
        _ => "later",
    }
}

fn freeze_error(e: &FreezeError) -> &'static str {
    match e {
        FreezeError::Field(..) => "Field",
        FreezeError::Tree(..) => "Tree",
        FreezeError::TooLarge => "TooLarge",
        _ => "later",
    }
}

fn format_error(e: &FormatError) -> &'static str {
    match e {
        FormatError::Misaligned => "Misaligned",
        FormatError::Truncated(..) => "Truncated",
        FormatError::BadMagic(..) => "BadMagic",
        FormatError::UnsupportedVersion(..) => "UnsupportedVersion",
        FormatError::WrongValueType { .. } => "WrongValueType",
        FormatError::Inconsistent(..) => "Inconsistent",
        FormatError::Shared => "Shared",
        FormatError::OtherTopology => "OtherTopology",
        _ => "later",
    }
}

fn meta_error(e: &MetaError) -> &'static str {
    match e {
        MetaError::EmptyUnit => "EmptyUnit",
        MetaError::BadVoxelSize => "BadVoxelSize",
        MetaError::BadTransform => "BadTransform",
        _ => "later",
    }
}

fn cache_error(e: &CacheError) -> &'static str {
    match e {
        CacheError::Io(..) => "Io",
        CacheError::Freeze(..) => "Freeze",
        CacheError::Meta(..) => "Meta",
        CacheError::Tree(..) => "Tree",
        CacheError::BadMagic(..) => "BadMagic",
        CacheError::UnsupportedVersion(..) => "UnsupportedVersion",
        CacheError::Truncated(..) => "Truncated",
        CacheError::Inconsistent(..) => "Inconsistent",
        CacheError::BadText(..) => "BadText",
        CacheError::Grid { .. } => "Grid",
        CacheError::NoGrid(..) => "NoGrid",
        CacheError::TooLarge => "TooLarge",
        CacheError::ReservedKey(..) => "ReservedKey",
        CacheError::FileTooLarge { .. } => "FileTooLarge",
        _ => "later",
    }
}

fn rule(e: &Rule) -> &'static str {
    match e {
        Rule::Sign => "Sign",
        Rule::NoCrossing => "NoCrossing",
        Rule::Fraction => "Fraction",
        Rule::NormalLength => "NormalLength",
        Rule::NormalInward => "NormalInward",
        Rule::UnnamedGroup => "UnnamedGroup",
        Rule::Owner => "Owner",
        _ => "later",
    }
}

fn hermite_error(e: &HermiteError) -> &'static str {
    match e {
        HermiteError::Field(..) => "Field",
        HermiteError::Tree(..) => "Tree",
        HermiteError::Cache(..) => "Cache",
        HermiteError::Format(..) => "Format",
        HermiteError::Violation { .. } => "Violation",
        HermiteError::OtherTopology(..) => "OtherTopology",
        HermiteError::OtherMeta(..) => "OtherMeta",
        HermiteError::Missing(..) => "Missing",
        HermiteError::Twice(..) => "Twice",
        HermiteError::UnknownField(..) => "UnknownField",
        HermiteError::Version(..) => "Version",
        HermiteError::BadGroupName(..) => "BadGroupName",
        HermiteError::BadGroupKey(..) => "BadGroupKey",
        HermiteError::ReservedKey(..) => "ReservedKey",
        HermiteError::ClaimsSdf => "ClaimsSdf",
        _ => "later",
    }
}

fn slice_error(e: &SliceError) -> &'static str {
    match e {
        SliceError::Field(..) => "Field",
        SliceError::EmptyRange => "EmptyRange",
        SliceError::TooLarge => "TooLarge",
        #[cfg(feature = "slice-png")]
        SliceError::BadColorMap(..) => "BadColorMap",
        #[cfg(feature = "slice-png")]
        SliceError::Encode(..) => "Encode",
        _ => "later",
    }
}

fn vtk_error(e: &VtkError) -> &'static str {
    match e {
        VtkError::Field(..) => "Field",
        VtkError::Tree(..) => "Tree",
        VtkError::Name(..) => "Name",
        VtkError::Infinite { .. } => "Infinite",
        VtkError::Inconsistent => "Inconsistent",
        VtkError::Io(..) => "Io",
        _ => "later",
    }
}

fn ghost_width(e: &GhostWidth) -> &'static str {
    match e {
        GhostWidth::One => "One",
        GhostWidth::Two => "Two",
        GhostWidth::Three => "Three",
        _ => "later",
    }
}

fn grid_error(e: &GridError) -> &'static str {
    match e {
        GridError::Tree(..) => "Tree",
        GridError::Field(..) => "Field",
        _ => "later",
    }
}

#[cfg(feature = "slice-png")]
fn color_map(e: &ColorMap) -> &'static str {
    match e {
        ColorMap::Diverging { .. } => "Diverging",
        ColorMap::Sequential { .. } => "Sequential",
        _ => "later",
    }
}

#[test]
fn every_enum_that_may_grow_is_matched_with_a_wildcard() {
    assert_eq!(analysis_error(&AnalysisError::TooSpread), "TooSpread");
    assert_eq!(tree_error(&TreeError::TooManyTiles), "TooManyTiles");
    assert_eq!(field_error(&FieldError::StaleField), "StaleField");
    assert_eq!(freeze_error(&FreezeError::TooLarge), "TooLarge");
    assert_eq!(format_error(&FormatError::Misaligned), "Misaligned");
    let empty_unit = GridMeta::new("g", [1.0; 3], "").unwrap_err();
    assert_eq!(meta_error(&empty_unit), "EmptyUnit");
    let not_a_file = CacheFile::from_bytes(&[0; 32]).unwrap_err();
    assert_eq!(cache_error(&not_a_file), "BadMagic");
    assert_eq!(rule(&Rule::Owner), "Owner");
    assert_eq!(hermite_error(&HermiteError::Missing("sign")), "Missing");
    assert_eq!(slice_error(&SliceError::EmptyRange), "EmptyRange");
    assert_eq!(vtk_error(&VtkError::Inconsistent), "Inconsistent");
    assert_eq!(ghost_width(&GhostWidth::Two), "Two");
    assert_eq!(grid_error(&GridError::Tree(TreeError::TooManyTiles)), "Tree");
    #[cfg(feature = "slice-png")]
    assert_eq!(color_map(&ColorMap::Diverging { limit: 1.0 }), "Diverging");
}

#[test]
fn every_struct_a_user_builds_is_built_by_its_constructor_and_set_by_assignment() {
    let mut meta = GridMeta::new("density", [0.5; 3], "mm").unwrap();
    meta.name = "pressure".to_owned();
    meta.transform[0][3] = 2.0;
    meta.extra.insert("source".to_owned(), "test".to_owned());
    assert!(meta.check().is_ok());
    assert_eq!((meta.name.as_str(), meta.voxel_size[1]), ("pressure", 0.5));
    #[cfg(feature = "slice-png")]
    {
        let mut slice = Slice::new(Axis::Z, 0, [0, 0], [7, 7]);
        slice.position = 3;
        assert_eq!((slice.position, slice.max), (3, [7, 7]));
        let image = Image::new(1, 1, vec![0, 0, 0, 255]);
        assert_eq!((image.width, image.height, image.rgba.len()), (1, 1, 4));
    }
}

/// A value type of a user's own, two `u16`, as `spikes/value/` used one.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::Immutable,
    zerocopy::KnownLayout,
)]
#[repr(C)]
struct Material {
    id: u16,
    flags: u16,
}

impl brodo::field::Value for Material {
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

#[test]
fn a_value_type_of_a_users_own_serves_in_memory() {
    // `Value` is open: a field of it is made, updated in parallel and labelled; only a
    // `Stored` type, brodo's, goes into a frozen grid or a file (ADR-0029).
    let empty = brodo::topology::Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    e.activate([16, 0, 0]).unwrap();
    let (t, _) = e.commit().unwrap();
    let mut f = brodo::field::Field::new(&t, Material { id: 0, flags: 0 });
    brodo::parallel::par_update(&t, &mut f, |n, values| {
        let id = u16::try_from(n.get()).unwrap() + 1;
        values.fill(Material { id, flags: 1 });
        Ok::<(), FieldError>(())
    })
    .unwrap();
    assert_eq!(f.value_at(&t, [17, 1, 1]).unwrap(), Material { id: 2, flags: 1 });
    let labels =
        brodo::analysis::label(&t, &f, |m| m.flags == 1, brodo::analysis::Connectivity::TwentySix)
            .unwrap();
    assert_eq!(labels.count(), 2);
}
