//! The crate's error types.

/// Why a mesh file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// Neither a binary STL, whose length its triangle count gives, nor an ASCII one.
    #[error("not an STL file: {0}")]
    NotStl(&'static str),
    /// The text is not UTF-8.
    #[error("the text is not UTF-8")]
    NotText,
    /// A word was expected and another found, or the text ended.
    #[error("expected {expected} at word {at}, found {found:?}")]
    Unexpected {
        /// What was expected.
        expected: &'static str,
        /// The word's place in the text, from 0.
        at: usize,
        /// What was found; empty at the end.
        found: String,
    },
    /// A number did not parse, or is not finite.
    #[error("not a finite number at word {at}: {found:?}")]
    BadNumber {
        /// The word's place in the text, from 0.
        at: usize,
        /// The word.
        found: String,
    },
    /// A loop held other than three vertices: only triangles are read.
    #[error("a loop at word {at} has {vertices} vertices; only triangles are read")]
    NotATriangle {
        /// The loop's place in the text, from 0.
        at: usize,
        /// The vertices it held.
        vertices: usize,
    },
    /// A binary file's coordinate is not finite.
    #[error("triangle {0} has a coordinate that is not finite")]
    NotFinite(u64),
    /// An OBJ statement colino does not read and cannot skip: free-form geometry, which
    /// defines shape it does not triangulate, or `call` and `csh`, which it never runs.
    #[error("line {line}: the {word:?} statement is not read")]
    Unsupported {
        /// The line, from 1.
        line: usize,
        /// The statement's keyword.
        word: String,
    },
    /// An OBJ statement unknown to the format.
    #[error("line {line}: unknown statement {word:?}")]
    UnknownStatement {
        /// The line, from 1.
        line: usize,
        /// The word.
        word: String,
    },
    /// A number on an OBJ line did not parse, or is not finite.
    #[error("line {line}: not a finite number: {found:?}")]
    BadNumberOnLine {
        /// The line, from 1.
        line: usize,
        /// The word.
        found: String,
    },
    /// A vertex with other than 3, 4 or 6 numbers.
    #[error("line {line}: a vertex has {numbers} numbers")]
    BadVertex {
        /// The line, from 1.
        line: usize,
        /// The numbers it has.
        numbers: usize,
    },
    /// A face with fewer than three vertices, or a vertex reference that is not a number.
    #[error("line {line}: a face needs three or more vertex numbers")]
    BadFace {
        /// The line, from 1.
        line: usize,
    },
    /// A face names vertex 0, or one the file does not hold.
    #[error("line {line}: no vertex {index}")]
    NoVertex {
        /// The line, from 1.
        line: usize,
        /// The number as written.
        index: i64,
    },
    /// A 3MF package is not a ZIP archive, lacks a part it needs, or holds a part that does
    /// not expand to what its directory says.
    #[error("3MF package: {0}")]
    Package(&'static str),
    /// A 3MF part is compressed by a method other than storing or deflate.
    #[error("3MF part compressed by {0}, not stored or deflated")]
    Compression(String),
    /// A 3MF part is not well-formed XML.
    #[error("XML at byte {at}: {reason}")]
    Xml {
        /// The byte in the part.
        at: u64,
        /// What the parser found.
        reason: String,
    },
    /// A 3MF part breaks the format, or uses what this reader refuses.
    #[error("3MF at byte {at}: {reason}")]
    Model {
        /// The byte in the part.
        at: u64,
        /// What is wrong.
        reason: &'static str,
    },
    /// An element in the 3MF core namespace the format does not define.
    #[error("3MF at byte {at}: unknown element {name:?}")]
    UnknownElement {
        /// The byte in the part.
        at: u64,
        /// The element's name.
        name: String,
    },
    /// The model requires extensions this reader does not support.
    #[error("3MF requires extensions {0:?}, which are not read")]
    RequiredExtension(String),
    /// A component or build item names an object not defined before it.
    #[error("3MF at byte {at}: no object {id} defined before")]
    NoObject {
        /// The byte in the part.
        at: u64,
        /// The id named.
        id: u64,
    },
    /// The mesh holds more than the limits allow.
    #[error("more {what} than the limit of {limit}")]
    TooLarge {
        /// Triangles or vertices.
        what: &'static str,
        /// The limit given.
        limit: u64,
    },
}

/// Why a mesh could not be worked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MeshError {
    /// A triangle names a vertex the mesh does not hold.
    #[error("triangle {triangle} names vertex {index}, beyond the mesh's vertices")]
    BadIndex {
        /// The triangle's place.
        triangle: usize,
        /// The index it names.
        index: u32,
    },
    /// More vertices than a `u32` numbers.
    #[error("more vertices than a u32 numbers")]
    TooManyVertices,
    /// `groups` is neither empty nor one per triangle.
    #[error("{groups} groups for {triangles} triangles")]
    GroupCount {
        /// The groups given.
        groups: usize,
        /// The triangles.
        triangles: usize,
    },
    /// A triangle's group has no name.
    #[error("triangle {triangle} is in group {group}, which has no name")]
    UnnamedGroup {
        /// The triangle's place.
        triangle: usize,
        /// Its group.
        group: u32,
    },
    /// Group 0 is named other than `""`.
    #[error("group 0 is named, but it is no group")]
    GroupZeroNamed,
    /// A triangle's vertex has a coordinate that is not finite: NaN or an infinity.
    #[error("vertex {vertex} has a coordinate that is not finite")]
    NotFinite {
        /// The vertex's index.
        vertex: u32,
    },
}

/// Why a mesh could not be converted.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ConvertError {
    /// The mesh names a vertex it lacks, or holds one that is not finite.
    #[error(transparent)]
    Mesh(#[from] MeshError),
    /// The tree refused an edit.
    #[error(transparent)]
    Tree(#[from] brodo::error::TreeError),
    /// A field refused a read or write.
    #[error(transparent)]
    Field(#[from] brodo::error::FieldError),
    /// The grid's metadata was refused.
    #[error(transparent)]
    Meta(#[from] brodo::error::MetaError),
    /// An option outside what conversion takes.
    #[error("bad option: {0}")]
    BadOption(&'static str),
    /// Neither the mesh nor the options give a unit.
    #[error("no unit: the mesh states none and the options give none")]
    NoUnit,
    /// The mesh reaches beyond the voxel coordinates brodo holds.
    #[error("the mesh reaches beyond the grid's i32 voxel coordinates")]
    OutOfRange,
}

/// Why Hermite data could not be made (ADR-0030).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HermiteError {
    /// The mesh names a vertex it lacks, holds one that is not finite, or its groups do not
    /// fit it.
    #[error(transparent)]
    Mesh(#[from] MeshError),
    /// A field refused a read or write.
    #[error(transparent)]
    Field(#[from] brodo::error::FieldError),
    /// The set broke brodo's format (ADR-0032), which colino's own checks should prevent.
    #[error(transparent)]
    Format(#[from] brodo::error::HermiteError),
    /// The grid's transform is not the axis-aligned one conversion writes.
    #[error("the grid's transform is not axis-aligned with positive voxel sizes")]
    Transform,
    /// The winding number's inside changes beyond the band, across an edge no leaf point
    /// owns: a badly oriented mesh, whose sign the winding number cannot give (B-016).
    #[error("the signs change beyond the band, at point {0:?}: is the mesh badly oriented?")]
    SignOutsideBand(brodo::tree::Coord),
}

/// Why a topology report could not be made.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ReportError {
    /// The mesh names a vertex it lacks, or holds one that is not finite.
    #[error(transparent)]
    Mesh(#[from] MeshError),
    /// Labelling or the Euler characteristic was refused.
    #[error(transparent)]
    Analysis(#[from] brodo::analysis::AnalysisError),
    /// The grid's transform is not the axis-aligned one conversion writes.
    #[error("the grid's transform is not axis-aligned with positive voxel sizes")]
    Transform,
}
