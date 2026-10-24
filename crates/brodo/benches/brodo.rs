//! Benchmark tier (TESTING.md §1) for brodo, on the shared harness (`harness/mod.rs`,
//! ADR-0019): `cargo bench -p brodo --bench brodo`, `--record` to record a baseline,
//! `--record NAME` one item's.
//!
//! The workload is a narrow-band sphere SDF of radius 80, the textbook |p| − r clamped to a
//! band of ±3: a leaf where the band passes, an inside tile where all of a leaf's region
//! lies below it. The file items read and write through the operating system's page cache,
//! which the harness's eviction buffer does not empty.

use std::path::PathBuf;

use brodo::accessor::ValueAccessor;
use brodo::cache::{self, CacheEntry, CacheFile};
use brodo::field::Field;
use brodo::frozen::freeze;
use brodo::gather::{GATHER_VOXELS, GhostWidth, gather, gather_wide};
use brodo::meta::GridMeta;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

mod harness;
use harness::{Failure, Item, repeated};

/// The sphere's radius, and the band its SDF is clamped to.
const RADIUS: i32 = 80;
const BAND: f32 = 3.0;
/// Voxels read by the random-access items.
const LOOKUPS: usize = 100_000;

fn out_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/bench/brodo")
}

fn main() -> Result<(), Failure> {
    harness::run(env!("CARGO_MANIFEST_DIR"), "brodo", Data::new, Data::items)
}

/// The sphere's leaf and tile origins.
fn sphere_regions() -> (Vec<Coord>, Vec<Coord>) {
    let lim = RADIUS + 16;
    let (mut leaves, mut tiles) = (Vec::new(), Vec::new());
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let ds = (0..512).map(|i| distance([x + i % 8, y + i / 8 % 8, z + i / 64]));
                let (lo, hi) = ds.fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)));
                if hi <= -BAND {
                    tiles.push([x, y, z]);
                } else if lo < BAND {
                    leaves.push([x, y, z]);
                }
            }
        }
    }
    (leaves, tiles)
}

fn distance(v: Coord) -> f32 {
    let [x, y, z] = v.map(f64::from);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a distance within the box, far below f32::MAX"
    )]
    let d = ((x * x + y * y + z * z).sqrt() - f64::from(RADIUS)) as f32;
    d
}

fn sphere_topology(leaves: &[Coord], tiles: &[Coord]) -> Result<Topology, Failure> {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in leaves {
        e.activate(*v)?;
    }
    for v in tiles {
        e.add_tile(TileLevel::Lower, *v)?;
    }
    Ok(e.commit()?.0)
}

fn sphere_field(t: &Topology) -> Result<Field<f32>, Failure> {
    let mut f = Field::new(t, BAND);
    par_update(t, &mut f, |n, values| -> Result<(), Failure> {
        for (voxel, v) in t.leaf_voxels(n)?.zip(values.iter_mut()) {
            *v = distance(voxel).clamp(-BAND, BAND);
        }
        Ok(())
    })?;
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(t, n, -BAND)?;
    }
    Ok(f)
}

/// Voxels spread over the sphere's box by a fixed linear congruential sequence.
fn random_voxels() -> Vec<Coord> {
    let side = 2 * (RADIUS + 16);
    let mut g = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        g = g.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        i32::try_from((g >> 33) % u64::try_from(side).unwrap_or(1)).unwrap_or(0) - side / 2
    };
    (0..LOOKUPS).map(|_| [next(), next(), next()]).collect()
}

/// The data every item reads, built once per child.
struct Data {
    leaves: Vec<Coord>,
    tiles: Vec<Coord>,
    t: Topology,
    f: Field<f32>,
    voxels: Vec<Coord>,
    entry: CacheEntry,
    file: PathBuf,
}

impl Data {
    fn new() -> Result<Self, Failure> {
        let (leaves, tiles) = sphere_regions();
        let t = sphere_topology(&leaves, &tiles)?;
        let f = sphere_field(&t)?;
        let entry = CacheEntry::new(&GridMeta::new("sdf", [1.0; 3], "voxel")?, &t, &f)?;
        let file = out_dir().join("sphere.cache");
        cache::write(&file, std::slice::from_ref(&entry))?;
        Ok(Self { leaves, tiles, t, f, voxels: random_voxels(), entry, file })
    }

    /// The items of the Benchmark tier (TESTING.md §1).
    fn items(&self) -> Vec<Item<'_>> {
        let (t, f) = (&self.t, &self.f);
        let items: Vec<Item<'_>> = vec![
            repeated(
                "activation",
                1,
                Box::new(|| Ok(sphere_topology(&self.leaves, &self.tiles)?.leaf_count() as f64)),
            ),
            repeated(
                "random_access_accessor",
                1,
                Box::new(|| {
                    let mut acc = ValueAccessor::new(t, f)?;
                    Ok(self.voxels.iter().map(|v| f64::from(acc.get(*v))).sum())
                }),
            ),
            repeated(
                "random_access_value_at",
                1,
                Box::new(|| self.voxels.iter().map(|v| Ok(f64::from(f.value_at(t, *v)?))).sum()),
            ),
            repeated(
                "leaf_parallel_update",
                32,
                Box::new(|| {
                    let mut out = Field::new(t, 0.0_f32);
                    par_update(t, &mut out, |n, values| -> Result<(), Failure> {
                        for (o, v) in values.iter_mut().zip(f.leaf(t, n)?) {
                            *o = 2.0 * v + 1.0;
                        }
                        Ok(())
                    })?;
                    Ok(f64::from(out.background()))
                }),
            ),
        ];
        items.into_iter().chain(self.gather_items()).chain(self.file_items()).collect()
    }

    /// The gathers: one ghost layer into an array, and three into a `Vec` (ADR-0015).
    fn gather_items(&self) -> Vec<Item<'_>> {
        let (t, f) = (&self.t, &self.f);
        vec![
            repeated(
                "gather",
                1,
                Box::new(|| {
                    let mut g = [0.0_f32; GATHER_VOXELS];
                    let mut sum = 0.0;
                    for n in t.leaf_numbers() {
                        gather(t, f, n, &mut g)?;
                        sum += f64::from(g[GATHER_VOXELS / 2]);
                    }
                    Ok(sum)
                }),
            ),
            repeated(
                "gather_w3",
                1,
                Box::new(|| {
                    let mut g = Vec::new();
                    let mut sum = 0.0;
                    for n in t.leaf_numbers() {
                        gather_wide(t, f, n, GhostWidth::Three, &mut g)?;
                        sum += f64::from(g[GhostWidth::Three.voxels() / 2]);
                    }
                    Ok(sum)
                }),
            ),
        ]
    }

    /// `freeze` and the cache profile's file I/O.
    fn file_items(&self) -> Vec<Item<'_>> {
        vec![
            repeated(
                "freeze",
                8,
                Box::new(|| Ok(freeze(&self.t, &self.f)?.as_bytes().len() as f64)),
            ),
            repeated(
                "cache_write",
                8,
                Box::new(|| {
                    cache::write(&self.file, std::slice::from_ref(&self.entry))?;
                    Ok(0.0)
                }),
            ),
            repeated(
                "cache_read",
                16,
                Box::new(|| Ok(CacheFile::read(&self.file, u64::MAX)?.grid_count() as f64)),
            ),
            repeated(
                "cache_map",
                256,
                Box::new(|| Ok(CacheFile::map(&self.file)?.grid_count() as f64)),
            ),
        ]
    }
}
