//! Paths through code that rests on unsafe code in brodo's dependencies, on a grid of a few
//! leaves, small enough for Miri (TESTING.md T7, ADR-0008): reading a frozen grid in place
//! (`zerocopy`), reading a cache file's bytes (`zerocopy`), and the leaf-parallel update and
//! reduction (`rayon`) on four threads. They run in the gate as any test; before a milestone
//! closes, Miri runs them (TESTING.md §3). Each value is checked against the field it came
//! from, so a path that reads the wrong bytes fails here as well as under Miri.

use brodo::cache::{CacheEntry, CacheFile, to_bytes};
use brodo::field::Field;
use brodo::frozen::freeze;
use brodo::gather::{GATHER_VOXELS, gather};
use brodo::meta::GridMeta;
use brodo::parallel::{par_reduce, par_update};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use brodo::view::GridView;

/// Three leaves, two of them neighbours, a leaf-sized tile and an upper-level tile, each
/// voxel and tile holding a distinct value.
fn grid() -> (Topology, Field<f32>) {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in [[0, 0, 0], [8, 0, 0], [-16, 8, 24]] {
        e.activate(v).unwrap();
    }
    e.add_tile(TileLevel::Lower, [0, 8, 0]).unwrap();
    e.add_tile(TileLevel::Upper, [128, 0, 0]).unwrap();
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, -1.0_f32);
    let mut k = 0.0;
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        for v in f.leaf_mut(&t, n).unwrap() {
            *v = k;
            k += 1.0;
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, k).unwrap();
        k += 1.0;
    }
    (t, f)
}

/// Every leaf voxel, each tile's origin, and a voxel of the background.
fn probes(t: &Topology) -> Vec<Coord> {
    let mut out: Vec<Coord> = t.leaf_numbers().flat_map(|n| t.leaf_voxels(n).unwrap()).collect();
    out.extend(t.tile_numbers().map(|n| t.tile(n).unwrap().1));
    out.push([-500, 0, 0]);
    out
}

fn reads_as(view: &GridView<'_, f32>, t: &Topology, f: &Field<f32>) {
    for v in probes(t) {
        assert_eq!(view.get(v).unwrap().to_bits(), f.value_at(t, v).unwrap().to_bits(), "at {v:?}");
    }
    for (k, n) in (0_u32..).zip(t.leaf_numbers()) {
        let bits = |l: &[f32; 512]| l.map(f32::to_bits);
        assert_eq!(bits(view.leaf(k).unwrap()), bits(f.leaf(t, n).unwrap()));
    }
}

#[test]
fn miri_a_frozen_grid_reads_in_place() {
    let (t, f) = grid();
    let frozen = freeze(&t, &f).unwrap();
    reads_as(&frozen.view::<f32>().unwrap(), &t, &f);
}

#[test]
fn miri_a_cache_files_bytes_read_in_place() {
    let (t, f) = grid();
    let meta = GridMeta::new("probe", [1.0; 3], "voxel").unwrap();
    let bytes = to_bytes(&[CacheEntry::new(&meta, &t, &f).unwrap()]).unwrap();
    let file = CacheFile::from_bytes(&bytes).unwrap();
    assert_eq!(file.meta(0).unwrap(), &meta);
    reads_as(&file.view::<f32>(0).unwrap(), &t, &f);
}

#[test]
fn miri_the_leaf_parallel_update_and_reduction_run_on_four_threads() {
    let (t, f) = grid();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    let (next, sum) = pool.install(|| {
        let mut next = Field::new(&t, 0.0_f32);
        par_update(&t, &mut next, |n, out| {
            let mut g = [0.0_f32; GATHER_VOXELS];
            gather(&t, &f, n, &mut g)?;
            for (i, u) in out.iter_mut().enumerate() {
                let c = (i % 8 + 1) + 10 * (i / 8 % 8 + 1) + 100 * (i / 64 + 1);
                *u = g[c] + g[c + 1];
            }
            Ok::<(), brodo::error::FieldError>(())
        })
        .unwrap();
        let sum = par_reduce(
            &t,
            &next,
            0.0_f64,
            |_, l| l.iter().map(|x| f64::from(*x)).sum(),
            |a, b| a + b,
        );
        (next, sum.unwrap())
    });
    let mut want = 0.0_f64;
    for n in t.leaf_numbers() {
        for (i, (v, got)) in t.leaf_voxels(n).unwrap().zip(next.leaf(&t, n).unwrap()).enumerate() {
            let expected =
                f.value_at(&t, v).unwrap() + f.value_at(&t, [v[0] + 1, v[1], v[2]]).unwrap();
            assert_eq!(got.to_bits(), expected.to_bits(), "leaf {n:?}, voxel {i}");
            want += f64::from(expected);
        }
    }
    assert_eq!(sum.to_bits(), want.to_bits());
}
