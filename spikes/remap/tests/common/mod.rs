//! What the scenarios share, so each candidate's test holds only its wiring to the API.
//! The diffusion is the explicit scheme for the heat equation along x; the spheres are
//! exact Euclidean distances, clamped to a band.

#![allow(dead_code)]

use remap_spike::core::{
    FieldData, LEAF_DIM, LEAF_VOXELS, Op, Region, Shape, Slot, SpikeError, Value, Voxel, voxel_of,
};

/// Diffusion number of the explicit scheme (stable below 0.5).
pub const D: f32 = 0.25;
/// Time steps of the diffusion scenario.
pub const STEPS: u32 = 6;

/// The diffusion scenario's first edit: three leaves on a line, one far leaf, one tile.
pub fn diffusion_initial() -> Vec<Op> {
    vec![
        Op::Activate([0, 0, 0]),
        Op::Activate([8, 0, 0]),
        Op::Activate([16, 0, 0]),
        Op::Activate([-40, 0, 0]),
        Op::AddTile([0, 3, 0]),
    ]
}

/// The edit after a step, if any: grow the line and split the tile after step 2; merge the
/// far leaf and grow again after step 4.
pub fn diffusion_edit(step: u32) -> Option<Vec<Op>> {
    match step {
        2 => Some(vec![Op::Activate([24, 0, 0]), Op::Split([0, 3, 0])]),
        4 => Some(vec![Op::Merge([-5, 0, 0]), Op::Activate([32, 0, 0])]),
        _ => None,
    }
}

/// The edit tried at the end, which the velocity's merge rule (M1) must refuse.
pub fn diffusion_refused() -> Vec<Op> {
    vec![Op::Merge([0, 0, 0])]
}

/// Initial values: temperature 1 in the first leaf, 2 in the tile; flag 1 in every leaf.
pub fn diffusion_init(shape: &Shape, temp: &mut FieldData<f32>, flag: &mut FieldData<u8>) {
    if let Slot::Leaf(i) = shape.slot([0, 0, 0]) {
        temp.leaf_mut(i).fill(1.0);
    }
    if let Slot::Tile(j) = shape.slot([0, 3, 0]) {
        temp.tile_values[j as usize] = 2.0;
    }
    flag.leaf_values.fill(1);
}

/// One explicit step of temperature, reading neighbours through the topology.
pub fn temp_kernel(shape: &Shape, temp: &FieldData<f32>, leaf: u32, out: &mut [f32]) {
    let r = shape.leaves[leaf as usize];
    for (i, o) in out.iter_mut().enumerate() {
        let v = voxel_of(r, i);
        let c = temp.value_at(shape, v);
        let l = temp.value_at(shape, [v[0] - 1, v[1], v[2]]);
        let h = temp.value_at(shape, [v[0] + 1, v[1], v[2]]);
        *o = c + D * (l - 2.0 * c + h);
    }
}

/// One step of velocity, reading two fields: relaxes and is pushed by temperature.
pub fn vel_kernel(
    vel: &FieldData<[f32; 3]>,
    temp: &FieldData<f32>,
    leaf: u32,
    out: &mut [[f32; 3]],
) {
    for ((o, v), t) in out.iter_mut().zip(vel.leaf(leaf)).zip(temp.leaf(leaf)) {
        *o = [v[0] * 0.5 + t, v[1], v[2]];
    }
}

/// Candidate M2 for temperature: the mean, summed in voxel order.
pub fn mean(values: &[f32]) -> Option<f32> {
    Some(values.iter().sum::<f32>() / LEAF_VOXELS as f32)
}

/// What a diffusion run ends with, compared across candidates.
pub struct Outcome {
    pub leaves: Vec<Region>,
    pub tiles: Vec<Region>,
    pub temp: FieldData<f32>,
    pub vel: FieldData<[f32; 3]>,
    pub flag: FieldData<u8>,
    pub refused: Result<(), SpikeError>,
}

/// Whether two fields hold the same bits.
pub fn same_field<T: Value>(a: &FieldData<T>, b: &FieldData<T>) -> bool {
    let same = |x: &[T], y: &[T]| x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.same_bits(q));
    same(&a.leaf_values, &b.leaf_values)
        && same(&a.tile_values, &b.tile_values)
        && a.background.same_bits(&b.background)
}

/// Checks what every candidate's diffusion run must end with, and that two runs agree.
pub fn check_outcome(o: &Outcome, other: &Outcome) {
    assert_eq!(o.refused, Err(SpikeError::NotUniform(0)), "M1 refuses the hot leaf's velocity");
    assert!(o.leaves.contains(&[0, 3, 0]), "the split tile is a leaf");
    assert!(o.tiles.contains(&[-5, 0, 0]), "the far leaf is merged");
    assert_eq!(o.leaves.len(), 6);
    assert_eq!((&o.leaves, &o.tiles), (&other.leaves, &other.tiles));
    assert!(same_field(&o.temp, &other.temp));
    assert!(same_field(&o.vel, &other.vel));
    assert!(same_field(&o.flag, &other.flag));
}

/// Half-width of the narrow band, in voxels.
pub const BAND: f32 = 3.0;
/// Tolerance on a stored distance against the exact one (TESTING.md T4): both are computed
/// the same way in `f32`, so any difference is a wrong value.
pub const SDF_TOL: f32 = 1e-6;

/// A sphere, in voxel units.
#[derive(Clone, Copy)]
pub struct Sphere {
    pub c: [f32; 3],
    pub r: f32,
}

pub const S1: Sphere = Sphere { c: [0.0, 0.0, 0.0], r: 20.0 };
pub const S2: Sphere = Sphere { c: [16.0, 4.0, 0.0], r: 14.0 };

/// Signed distance from a voxel centre to the sphere.
pub fn dist(s: Sphere, v: Voxel) -> f32 {
    let d = [0, 1, 2].map(|k| v[k] as f32 - s.c[k]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - s.r
}

/// A distance clamped to the band.
pub fn clamped(d: f32) -> f32 {
    d.clamp(-BAND, BAND)
}

/// Where a region lies against a sphere's band.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Inside,
    Band,
    Outside,
}

/// Classifies a region from its nearest and farthest voxels.
pub fn classify(s: Sphere, r: Region) -> Class {
    let lo = r.map(|c| (c * LEAF_DIM) as f32);
    let hi = lo.map(|c| c + (LEAF_DIM - 1) as f32);
    let mut near = 0.0;
    let mut far = 0.0;
    for k in 0..3 {
        let n = s.c[k].clamp(lo[k], hi[k]) - s.c[k];
        let f = (lo[k] - s.c[k]).abs().max((hi[k] - s.c[k]).abs());
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

/// Every region that may meet either sphere's band.
pub fn regions() -> Vec<Region> {
    let mut out = Vec::new();
    for z in -4..=4 {
        for y in -4..=4 {
            for x in -4..=5 {
                out.push([x, y, z]);
            }
        }
    }
    out
}

/// The first sphere, from an empty shape: band leaves and inside tiles.
pub fn sphere_ops() -> Vec<Op> {
    regions()
        .into_iter()
        .filter_map(|r| match classify(S1, r) {
            Class::Band => Some(Op::Activate(voxel_of(r, 0))),
            Class::Inside => Some(Op::AddTile(r)),
            Class::Outside => None,
        })
        .collect()
}

/// Fills the first sphere's values.
pub fn sphere_fill(shape: &Shape, f: &mut FieldData<f32>) {
    for (i, r) in shape.leaves.iter().enumerate() {
        for (k, v) in f.leaf_mut(i as u32).iter_mut().enumerate() {
            *v = clamped(dist(S1, voxel_of(*r, k)));
        }
    }
    f.tile_values.fill(-BAND);
}

/// The second sphere's edit: its band becomes leaves (splitting tiles), its inside tiles.
pub fn union_ops(shape: &Shape) -> Vec<Op> {
    regions()
        .into_iter()
        .filter_map(|r| match (classify(S2, r), shape.slot(r)) {
            (Class::Band, Slot::Leaf(_)) => None,
            (Class::Band, Slot::Tile(_) | Slot::Background) => Some(Op::Activate(voxel_of(r, 0))),
            (Class::Inside, Slot::Background) => Some(Op::AddTile(r)),
            (Class::Inside, Slot::Leaf(_) | Slot::Tile(_)) | (Class::Outside, _) => None,
        })
        .collect()
}

/// The union's value in one leaf, in place.
pub fn union_kernel(shape: &Shape, leaf: u32, out: &mut [f32]) {
    let r = shape.leaves[leaf as usize];
    for (k, v) in out.iter_mut().enumerate() {
        *v = v.min(clamped(dist(S2, voxel_of(r, k))));
    }
}

/// Tiles added inside the second sphere took the background; they are inside.
pub fn union_tiles(f: &mut FieldData<f32>) {
    f.tile_values.fill(-BAND);
}

/// Leaves wholly inside the second sphere, now uniform, merged into tiles.
pub fn merge_ops(shape: &Shape) -> Vec<Op> {
    shape.leaves.iter().filter(|r| classify(S2, **r) == Class::Inside).map(|r| Op::Merge(*r)).collect()
}

/// Checks a union against exact distances: leaves voxel by voxel, tiles and the background
/// over every voxel they stand for. Returns the number of merged leaves seen.
pub fn check_union(shape: &Shape, f: &FieldData<f32>) {
    for r in regions() {
        for k in 0..LEAF_VOXELS {
            let v = voxel_of(r, k);
            let exact = clamped(dist(S1, v).min(dist(S2, v)));
            let got = f.value_at(shape, v);
            assert!((got - exact).abs() <= SDF_TOL, "{v:?}: {got} against {exact}");
        }
    }
    assert!(!shape.tiles.is_empty() && !shape.leaves.is_empty());
}
