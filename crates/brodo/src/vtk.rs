//! Checking output (ARCHITECTURE.md §6.1, ADR-0018): a field written for ParaView as one
//! ASCII UnstructuredGrid (`.vtu`), from VTK's published file-format documentation.
//!
//! Each leaf voxel is a VTK_VOXEL cell of edge 1 and each tile one VTK_VOXEL cell of its
//! region's size, in index space. Points are shared between cells, in the order the cells
//! first use them; a cell's eight points run x fastest, then y, then z. Values are cell
//! data: a scalar field named by `CellData`'s `Scalars`, a field of three components, one
//! value's components to a line, by its `Vectors`. Cells are leaves by leaf number, each
//! leaf's voxels x fastest, then tiles by tile number, so the same input always gives the
//! same bytes.
//!
//! Points are `Float64`, which holds every `i32` coordinate exactly; connectivity and
//! offsets are `Int64`. VTK 9.6.2 reads `NaN` back as a NaN but `-inf` as `+inf`
//! (measured 2026-10-17, ADR-0018), so a NaN, or a NaN component, is written as `NaN` and a
//! value with an infinite component is refused.

use std::collections::BTreeMap;
use std::io::{BufWriter, Write};

use crate::error::VtkError;
use crate::field::{Field, Value};
use crate::topology::Topology;
use crate::tree::Coord;

/// VTK_VOXEL's cell type, from VTK's public Python API (ADR-0018).
const VOXEL: u8 = 11;

/// A point, in index space; `i64` so that a tile's far corner never overflows.
type Point = [i64; 3];

mod sealed {
    /// Keeps [`super::VtkValue`] to the types this module writes.
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
    impl Sealed for u8 {}
    impl Sealed for u32 {}
    impl Sealed for [f32; 3] {}
    impl Sealed for [f64; 3] {}
}

/// A value type with a VTK data-array type and a number of components (ADR-0018).
pub trait VtkValue: Value + sealed::Sealed {
    /// The `type` attribute of its `DataArray`.
    const TYPE_NAME: &'static str;

    /// Its components: 1 for a scalar, 3 for a vector.
    const COMPONENTS: u32;

    /// Whether VTK reads the value back as written: false for an infinity.
    fn readable(self) -> bool;

    /// Writes the value as ASCII, components apart by a space; a NaN as `NaN`.
    fn write_ascii(self, w: &mut dyn Write) -> std::io::Result<()>;
}

impl VtkValue for f32 {
    const TYPE_NAME: &'static str = "Float32";
    const COMPONENTS: u32 = 1;

    fn readable(self) -> bool {
        !self.is_infinite()
    }

    fn write_ascii(self, w: &mut dyn Write) -> std::io::Result<()> {
        // Display prints the shortest text that reads back to the same value.
        if self.is_nan() { w.write_all(b"NaN") } else { write!(w, "{self}") }
    }
}

impl VtkValue for f64 {
    const TYPE_NAME: &'static str = "Float64";
    const COMPONENTS: u32 = 1;

    fn readable(self) -> bool {
        !self.is_infinite()
    }

    fn write_ascii(self, w: &mut dyn Write) -> std::io::Result<()> {
        if self.is_nan() { w.write_all(b"NaN") } else { write!(w, "{self}") }
    }
}

impl VtkValue for u8 {
    const TYPE_NAME: &'static str = "UInt8";
    const COMPONENTS: u32 = 1;

    fn readable(self) -> bool {
        true
    }

    fn write_ascii(self, w: &mut dyn Write) -> std::io::Result<()> {
        write!(w, "{self}")
    }
}

impl VtkValue for u32 {
    const TYPE_NAME: &'static str = "UInt32";
    const COMPONENTS: u32 = 1;

    fn readable(self) -> bool {
        true
    }

    fn write_ascii(self, w: &mut dyn Write) -> std::io::Result<()> {
        write!(w, "{self}")
    }
}

/// Writes a vector's components through their scalar type, apart by a space.
fn write_components<S: VtkValue>(c: [S; 3], w: &mut dyn Write) -> std::io::Result<()> {
    c[0].write_ascii(w)?;
    for x in &c[1..] {
        w.write_all(b" ")?;
        x.write_ascii(w)?;
    }
    Ok(())
}

impl VtkValue for [f32; 3] {
    const TYPE_NAME: &'static str = "Float32";
    const COMPONENTS: u32 = 3;

    fn readable(self) -> bool {
        self.iter().all(|x| x.readable())
    }

    fn write_ascii(self, w: &mut dyn Write) -> std::io::Result<()> {
        write_components(self, w)
    }
}

impl VtkValue for [f64; 3] {
    const TYPE_NAME: &'static str = "Float64";
    const COMPONENTS: u32 = 3;

    fn readable(self) -> bool {
        self.iter().all(|x| x.readable())
    }

    fn write_ascii(self, w: &mut dyn Write) -> std::io::Result<()> {
        write_components(self, w)
    }
}

/// One cell: its lowest corner, its edge, and its value.
struct Cell<T> {
    origin: Coord,
    edge: i64,
    value: T,
}

impl<T> Cell<T> {
    /// Its eight corners, x fastest, then y, then z.
    fn corners(&self) -> impl Iterator<Item = Point> + use<'_, T> {
        let o = self.origin.map(i64::from);
        (0..8_i64).map(move |k| {
            [
                o[0] + (k & 1) * self.edge,
                o[1] + (k >> 1 & 1) * self.edge,
                o[2] + (k >> 2) * self.edge,
            ]
        })
    }
}

/// Writes `f` on `t` as one ASCII `.vtu`, its values named `name`.
///
/// `name` is one or more printable ASCII characters or spaces, none of `<>&"'`.
pub fn write_vtu<T: VtkValue, W: Write>(
    t: &Topology,
    f: &Field<T>,
    name: &str,
    out: W,
) -> Result<(), VtkError> {
    check_name(name)?;
    let cells = cells(t, f)?;
    let (ids, points) = share_points(&cells);
    let mut w = BufWriter::new(out);
    writeln!(w, r#"<?xml version="1.0"?>"#)?;
    writeln!(w, r#"<VTKFile type="UnstructuredGrid" version="0.1" byte_order="LittleEndian">"#)?;
    writeln!(w, "<UnstructuredGrid>")?;
    writeln!(w, r#"<Piece NumberOfPoints="{}" NumberOfCells="{}">"#, points.len(), cells.len())?;
    let (role, components) = match T::COMPONENTS {
        1 => ("Scalars", String::new()),
        n => ("Vectors", format!(r#" NumberOfComponents="{n}""#)),
    };
    writeln!(w, r#"<CellData {role}="{name}">"#)?;
    writeln!(w, r#"<DataArray type="{}" Name="{name}"{components} format="ascii">"#, T::TYPE_NAME)?;
    for c in &cells {
        c.value.write_ascii(&mut w)?;
        writeln!(w)?;
    }
    writeln!(w, "</DataArray>\n</CellData>\n<Points>")?;
    writeln!(w, r#"<DataArray type="Float64" NumberOfComponents="3" format="ascii">"#)?;
    for p in &points {
        writeln!(w, "{} {} {}", p[0], p[1], p[2])?;
    }
    writeln!(w, "</DataArray>\n</Points>\n<Cells>")?;
    write_cells(&mut w, &cells, &ids)?;
    writeln!(w, "</Cells>\n</Piece>\n</UnstructuredGrid>\n</VTKFile>")?;
    w.flush()?;
    Ok(())
}

/// Refuses a name that would need escaping in XML, or none.
fn check_name(name: &str) -> Result<(), VtkError> {
    let fits = |c: char| (c.is_ascii_graphic() || c == ' ') && !"<>&\"'".contains(c);
    if name.is_empty() || !name.chars().all(fits) {
        return Err(VtkError::Name(name.to_owned()));
    }
    Ok(())
}

/// The cells, leaves then tiles; an infinite value is refused before anything is written.
fn cells<T: VtkValue>(t: &Topology, f: &Field<T>) -> Result<Vec<Cell<T>>, VtkError> {
    let mut out = Vec::new();
    for n in t.leaf_numbers() {
        for (origin, value) in t.leaf_voxels(n)?.zip(f.leaf(t, n)?) {
            out.push(Cell { origin, edge: 1, value: *value });
        }
    }
    for n in t.tile_numbers() {
        let (level, origin) = t.tile(n)?;
        out.push(Cell { origin, edge: i64::from(level.edge()), value: f.tile(t, n)? });
    }
    match out.iter().find(|c| !c.value.readable()) {
        Some(c) => Err(VtkError::Infinite { origin: c.origin }),
        None => Ok(out),
    }
}

/// Each cell corner's point number, and the points in the order first used.
fn share_points<T>(cells: &[Cell<T>]) -> (BTreeMap<Point, u64>, Vec<Point>) {
    let mut ids = BTreeMap::new();
    let mut points = Vec::new();
    for p in cells.iter().flat_map(Cell::corners) {
        ids.entry(p).or_insert_with(|| {
            points.push(p);
            // Lossless: a usize fits a u64 on every target.
            points.len() as u64 - 1
        });
    }
    (ids, points)
}

/// The `Cells` element's three arrays: connectivity, offsets and types.
fn write_cells<T>(
    w: &mut impl Write,
    cells: &[Cell<T>],
    ids: &BTreeMap<Point, u64>,
) -> Result<(), VtkError> {
    writeln!(w, r#"<DataArray type="Int64" Name="connectivity" format="ascii">"#)?;
    for c in cells {
        let corners: Vec<String> = c
            .corners()
            .map(|p| ids.get(&p).map(u64::to_string).ok_or(VtkError::Inconsistent))
            .collect::<Result<_, _>>()?;
        writeln!(w, "{}", corners.join(" "))?;
    }
    writeln!(w, "</DataArray>")?;
    writeln!(w, r#"<DataArray type="Int64" Name="offsets" format="ascii">"#)?;
    for k in 1..=cells.len() {
        writeln!(w, "{}", k * 8)?;
    }
    writeln!(w, "</DataArray>")?;
    writeln!(w, r#"<DataArray type="UInt8" Name="types" format="ascii">"#)?;
    for _ in cells {
        writeln!(w, "{VOXEL}")?;
    }
    writeln!(w, "</DataArray>")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TileLevel;

    /// One tile of edge 8 at the given origin, holding `value`, on a background of zero.
    fn one_tile<T: VtkValue + Default>(origin: Coord, value: T) -> (Topology, Field<T>) {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.add_tile(TileLevel::Lower, origin).unwrap();
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, T::default());
        let n = t.tile_numbers().next().unwrap();
        f.set_tile(&t, n, value).unwrap();
        (t, f)
    }

    fn text<T: VtkValue>(t: &Topology, f: &Field<T>) -> String {
        let mut out = Vec::new();
        write_vtu(t, f, "d", &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_tile_is_one_voxel_cell_of_its_size() {
        let (t, f) = one_tile([8, -16, 0], 2.5_f32);
        let want = r#"<?xml version="1.0"?>
<VTKFile type="UnstructuredGrid" version="0.1" byte_order="LittleEndian">
<UnstructuredGrid>
<Piece NumberOfPoints="8" NumberOfCells="1">
<CellData Scalars="d">
<DataArray type="Float32" Name="d" format="ascii">
2.5
</DataArray>
</CellData>
<Points>
<DataArray type="Float64" NumberOfComponents="3" format="ascii">
8 -16 0
16 -16 0
8 -8 0
16 -8 0
8 -16 8
16 -16 8
8 -8 8
16 -8 8
</DataArray>
</Points>
<Cells>
<DataArray type="Int64" Name="connectivity" format="ascii">
0 1 2 3 4 5 6 7
</DataArray>
<DataArray type="Int64" Name="offsets" format="ascii">
8
</DataArray>
<DataArray type="UInt8" Name="types" format="ascii">
11
</DataArray>
</Cells>
</Piece>
</UnstructuredGrid>
</VTKFile>
"#;
        assert_eq!(text(&t, &f), want);
    }

    #[test]
    fn a_vector_is_three_components_named_as_vectors() {
        let (t, f) = one_tile([8, -16, 0], [0.5_f64, -0.0025, f64::NAN]);
        let s = text(&t, &f);
        let want = r#"<CellData Vectors="d">
<DataArray type="Float64" Name="d" NumberOfComponents="3" format="ascii">
0.5 -0.0025 NaN
</DataArray>
</CellData>"#;
        assert!(s.contains(want), "{s}");
        let (t, f) = one_tile([0, 0, 0], [1.25_f32, -2.0, 3.5]);
        assert!(text(&t, &f).contains("format=\"ascii\">\n1.25 -2 3.5\n</DataArray>"));
        for v in [[0.0, f32::NEG_INFINITY, 0.0], [0.0, 0.0, f32::INFINITY]] {
            let (t, f) = one_tile([0, 8, 0], v);
            let mut out = Vec::new();
            let got = write_vtu(&t, &f, "d", &mut out);
            assert!(matches!(got, Err(VtkError::Infinite { origin: [0, 8, 0] })), "{got:?}");
            assert!(out.is_empty());
        }
    }

    #[test]
    fn two_tiles_side_by_side_share_four_points() {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.add_tile(TileLevel::Lower, [0, 0, 0]).unwrap();
        e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
        let (t, _) = e.commit().unwrap();
        let s = text(&t, &Field::new(&t, 1_u8));
        assert!(s.contains(r#"<Piece NumberOfPoints="12" NumberOfCells="2">"#), "{s}");
        assert!(s.contains(r#"<DataArray type="UInt8" Name="d" format="ascii">"#), "{s}");
    }

    #[test]
    fn an_empty_grid_has_no_cell() {
        let t = Topology::new();
        let s = text(&t, &Field::new(&t, 0.0_f64));
        assert!(s.contains(r#"<Piece NumberOfPoints="0" NumberOfCells="0">"#), "{s}");
    }

    #[test]
    fn a_nan_is_written_as_nan() {
        let (t, f) = one_tile([0, 0, 0], -f64::NAN);
        assert!(text(&t, &f).contains("format=\"ascii\">\nNaN\n</DataArray>"));
    }

    #[test]
    fn an_infinity_is_refused_before_anything_is_written() {
        for value in [f32::INFINITY, f32::NEG_INFINITY] {
            let (t, f) = one_tile([0, 8, 0], value);
            let mut out = Vec::new();
            let got = write_vtu(&t, &f, "d", &mut out);
            assert!(matches!(got, Err(VtkError::Infinite { origin: [0, 8, 0] })), "{got:?}");
            assert!(out.is_empty());
        }
        let empty = Topology::new();
        let mut e = empty.edit();
        e.activate([0, 0, 0]).unwrap();
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, 0.0_f64);
        let n = t.leaf_numbers().next().unwrap();
        f.leaf_mut(&t, n).unwrap()[8 + 64] = f64::INFINITY;
        let got = write_vtu(&t, &f, "d", Vec::new());
        assert!(matches!(got, Err(VtkError::Infinite { origin: [0, 1, 1] })), "{got:?}");
    }

    #[test]
    fn a_name_needing_escapes_or_empty_is_refused() {
        let (t, f) = one_tile([0, 0, 0], 1_u32);
        for name in ["", "a<b", "a&b", "a\"b", "a'b", "tab\t", "é"] {
            let got = write_vtu(&t, &f, name, Vec::new());
            assert!(matches!(got, Err(VtkError::Name(ref n)) if n == name), "{name:?}: {got:?}");
        }
        assert!(write_vtu(&t, &f, "signed distance_2", Vec::new()).is_ok());
    }
}
