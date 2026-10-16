//! A narrow-band signed distance field of a sphere, and the union of two, on one grid
//! (ROADMAP.md, P1 slice).
//!
//! Textbook material only (AGENTS.md S1): the signed distance to a sphere is |x − c| − r;
//! the union of two solids has the smaller of their signed distances. The field is clamped
//! to a band of ±3 voxels: leaves where the band passes, tiles at −3 inside, the background
//! at +3 outside. The union is written here, in the example (ADR-0012).
//!
//! The second sphere's band crosses the first one's inside, so its tiles are split into
//! leaves that start from the tile's value. The check: every voxel of a box holding both
//! spheres has the bits of the clamped exact distance.

use std::error::Error;

use brodo::error::TreeError;
use brodo::grid::Grid;
use brodo::parallel::par_update;
use brodo::topology::{Location, TopologyEditor};
use brodo::tree::{Coord, TileLevel};

type BoxError = Box<dyn Error + Send + Sync>;

/// Half the band's width, in voxels.
const BAND: f32 = 3.0;

/// A sphere, in voxel units.
#[derive(Clone, Copy)]
struct Sphere {
    c: [f32; 3],
    r: f32,
}

const FIRST: Sphere = Sphere { c: [0.0, 0.0, 0.0], r: 20.0 };
const SECOND: Sphere = Sphere { c: [16.0, 4.0, 0.0], r: 14.0 };

/// The leaf-sized regions to consider, by origin: they hold both spheres and their bands.
const REGIONS: std::ops::Range<i32> = -4..6;

fn distance(s: Sphere, v: Coord) -> f32 {
    let d = [0, 1, 2].map(|k| v[k] as f32 - s.c[k]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - s.r
}

fn clamped(d: f32) -> f32 {
    d.clamp(-BAND, BAND)
}

/// Where a leaf-sized region lies against a sphere's band.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Inside,
    Band,
    Outside,
}

/// Classifies the region at `o` from its nearest and farthest voxels.
fn classify(s: Sphere, o: Coord) -> Class {
    let (mut near, mut far) = (0.0, 0.0);
    for (c, start) in s.c.iter().zip(o) {
        let (lo, hi) = (start as f32, (start + 7) as f32);
        let n = c.clamp(lo, hi) - c;
        let f = (lo - c).abs().max((hi - c).abs());
        near += n * n;
        far += f * f;
    }
    if far.sqrt() - s.r <= -BAND {
        Class::Inside
    } else if near.sqrt() - s.r >= BAND {
        Class::Outside
    } else {
        Class::Band
    }
}

/// Every leaf-sized region's origin.
fn regions() -> impl Iterator<Item = Coord> {
    REGIONS.flat_map(|z| REGIONS.flat_map(move |y| REGIONS.map(move |x| [x * 8, y * 8, z * 8])))
}

/// One edit of the topology.
#[derive(Clone, Copy)]
enum Edit {
    Split(Coord),
    Activate(Coord),
    AddTile(Coord),
}

/// The edits that add a sphere to a topology, decided from what each region holds now:
/// its band as leaves, splitting the tiles it meets, and its inside as tiles where nothing
/// is.
fn sphere_edits(s: Sphere, now: impl Fn(Coord) -> Location) -> Vec<Edit> {
    regions()
        .filter_map(|o| match (classify(s, o), now(o)) {
            (Class::Band, Location::Tile(..)) => Some(Edit::Split(o)),
            (Class::Band, Location::Background) => Some(Edit::Activate(o)),
            (Class::Inside, Location::Background) => Some(Edit::AddTile(o)),
            (Class::Band | Class::Inside, Location::Leaf(_))
            | (Class::Inside, Location::Tile(..))
            | (Class::Outside, _) => None,
        })
        .collect()
}

fn apply(e: &mut TopologyEditor<'_>, edits: &[Edit]) -> Result<(), TreeError> {
    for edit in edits {
        match *edit {
            Edit::Split(o) => e.split_tile(o)?,
            Edit::Activate(o) => {
                e.activate(o)?;
            }
            Edit::AddTile(o) => e.add_tile(TileLevel::Lower, o)?,
        }
    }
    Ok(())
}

/// Takes the union with a sphere in every leaf, and sets every tile inside.
fn union(g: &mut Grid<f32>, s: Sphere) -> Result<(), BoxError> {
    let (t, f) = g.parts_mut();
    par_update(t, f, |n, out| -> Result<(), BoxError> {
        let o = t.leaf_origin(n)?;
        for (i, v) in out.iter_mut().enumerate() {
            *v = v.min(clamped(distance(s, voxel(o, i))));
        }
        Ok(())
    })?;
    for n in t.tile_numbers() {
        f.set_tile(t, n, -BAND)?;
    }
    Ok(())
}

/// The voxel at index `i` of the leaf at `o`, x fastest.
#[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap, reason = "below 8")]
fn voxel(o: Coord, i: usize) -> Coord {
    let l = [i % 8, i / 8 % 8, i / 64].map(|c| c as i32);
    [o[0] + l[0], o[1] + l[1], o[2] + l[2]]
}

/// Builds the first sphere, then the union with the second.
fn build() -> Result<Grid<f32>, BoxError> {
    let mut g = Grid::new(BAND);
    for s in [FIRST, SECOND] {
        // Decided before the edit: inside `Grid::edit` the grid is borrowed by the editor.
        let edits = sphere_edits(s, |o| g.topology().locate(o));
        g.edit(|e| apply(e, &edits))?;
        union(&mut g, s)?;
    }
    Ok(g)
}

/// Checks every voxel of the regions against the clamped exact distance of the union, and
/// that the second sphere split some of the first one's tiles.
fn check(g: &Grid<f32>) -> Result<(), BoxError> {
    for o in regions() {
        for i in 0..512 {
            let v = voxel(o, i);
            let want = clamped(distance(FIRST, v).min(distance(SECOND, v)));
            let got = g.value_at(v)?;
            if got.to_bits() != want.to_bits() {
                return Err(format!("{v:?}: {got} instead of {want}").into());
            }
        }
    }
    let split = regions()
        .filter(|o| classify(FIRST, *o) == Class::Inside)
        .filter(|o| matches!(g.topology().locate(*o), Location::Leaf(_)))
        .count();
    if split == 0 {
        return Err("no tile of the first sphere was split".into());
    }
    Ok(())
}

fn main() -> Result<(), BoxError> {
    check(&build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_union_matches_exact_distances() {
        check(&build().unwrap()).unwrap();
    }
}
