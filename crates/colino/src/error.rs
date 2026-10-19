//! The crate's error types.

/// Why a mesh file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
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
}
