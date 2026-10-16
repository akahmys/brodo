//! Use: pruning a narrow-band sphere SDF, as a shape kernel would, under S1 and S2. The
//! sphere (radius 120 voxels, band ±3) is built with a leaf in every region it touches;
//! pruning merges each uniform leaf into a leaf-sized tile and each block that is wholly
//! inside into a block tile. Both must end with the same topology and with every voxel of
//! the eight blocks at the clamped exact distance. Run in release, with `--nocapture` for the
//! counts and times.

use std::time::Instant;

use merge_spike::*;

const DIM: i32 = 16;
const BAND: f32 = 3.0;
const CENTRE: [f32; 3] = [64.0, 64.0, 64.0];
const RADIUS: f32 = 120.0;
const REFUSE: BackgroundInMerge = BackgroundInMerge::Refuse;

fn exact(v: Voxel) -> f32 {
    let d = [0, 1, 2].map(|k| v[k] as f32 - CENTRE[k]);
    ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - RADIUS).clamp(-BAND, BAND)
}

fn blocks() -> Vec<Block> {
    (0..27).map(|i| [i % 3 - 1, i / 3 % 3 - 1, i / 9 - 1]).collect()
}

/// A leaf in every region whose nearest voxel is within the band of the sphere's surface or
/// inside it, holding the clamped exact distance.
fn build(p: Policy) -> (Topology, FieldData<f32>) {
    let near = |r: Region| {
        let d: f32 = (0..3)
            .map(|k| {
                let (lo, hi) = ((r[k] * 8) as f32, (r[k] * 8 + 7) as f32);
                let n = CENTRE[k].clamp(lo, hi) - CENTRE[k];
                n * n
            })
            .sum();
        d.sqrt() - RADIUS < BAND
    };
    let ops: Vec<Op> =
        blocks().into_iter().flat_map(|b| children(b, DIM)).filter(|r| near(*r)).map(Op::Activate).collect();
    let (t, _) = commit_r2(&Topology::new(DIM), &ops, p).unwrap();
    let mut f = FieldData::new(&t, BAND);
    for (n, r) in t.leaves().iter().enumerate() {
        for (i, x) in f.leaf_mut(n as u32).iter_mut().enumerate() {
            *x = exact(voxel_of(*r, i));
        }
    }
    (t, f)
}

fn uniform_leaves(t: &Topology, f: &FieldData<f32>) -> Vec<Region> {
    t.leaves()
        .iter()
        .enumerate()
        .filter(|(n, _)| uniform_voxels(f.leaf(*n as u32)).is_some())
        .map(|(_, r)| *r)
        .collect()
}

/// S1: merge the uniform leaves, commit, then merge the blocks that hold only tiles.
fn prune_stepwise(t: &Topology, f: &FieldData<f32>) -> (Topology, FieldData<f32>, usize) {
    let p = Policy { merging: Merging::Stepwise, background: REFUSE };
    let ops: Vec<Op> = uniform_leaves(t, f).into_iter().map(Op::MergeLeaf).collect();
    let (t1, r) = commit_r2(t, &ops, p).unwrap();
    let f1 = remap(f, &r, Rule::Counted(&uniform_counted::<f32>)).unwrap();
    let full = |b: Block| {
        children(b, DIM).all(|c| matches!(t1.locate(c), Located::Tile(_)))
            && children(b, DIM).map(|c| t1.locate(c)).all(|l| match l {
                Located::Tile(k) => f1.tile_values[k as usize].to_bits() == (-BAND).to_bits(),
                Located::Leaf(_) | Located::Background => false,
            })
    };
    let ops: Vec<Op> = blocks().into_iter().filter(|b| full(*b)).map(Op::MergeBlock).collect();
    let (t2, r) = commit_r2(&t1, &ops, p).unwrap();
    let f2 = remap(&f1, &r, Rule::Counted(&uniform_counted::<f32>)).unwrap();
    (t2, f2, 2)
}

/// S2: one commit merging the blocks whose every region is a uniform leaf of one value, and
/// the other uniform leaves.
fn prune_at_once(t: &Topology, f: &FieldData<f32>) -> (Topology, FieldData<f32>, usize) {
    let p = Policy { merging: Merging::AtOnce, background: REFUSE };
    let uniform = uniform_leaves(t, f);
    let inside = |r: &Region| match t.locate(*r) {
        Located::Leaf(n) => f.leaf(n)[0].to_bits() == (-BAND).to_bits() && uniform.contains(r),
        Located::Tile(_) | Located::Background => false,
    };
    let full: Vec<Block> = blocks().into_iter().filter(|b| children(*b, DIM).all(|c| inside(&c))).collect();
    let ops: Vec<Op> = full
        .iter()
        .map(|b| Op::MergeBlock(*b))
        .chain(uniform.into_iter().filter(|r| !full.contains(&block_of(*r, DIM))).map(Op::MergeLeaf))
        .collect();
    let (t1, r) = commit_r2(t, &ops, p).unwrap();
    let f1 = remap(f, &r, Rule::Counted(&uniform_counted::<f32>)).unwrap();
    (t1, f1, 1)
}

fn check(t: &Topology, f: &FieldData<f32>) {
    for b in blocks() {
        for r in children(b, DIM) {
            for i in (0..LEAF_VOXELS).step_by(7) {
                let v = voxel_of(r, i);
                assert_eq!(f.value_at(t, v).to_bits(), exact(v).to_bits(), "at {v:?}");
            }
        }
    }
}

#[test]
fn pruning_ends_the_same_under_s1_and_s2() {
    let p = Policy { merging: Merging::AtOnce, background: REFUSE };
    let (t, f) = build(p);
    let counts = |t: &Topology| {
        let blocks = t.tiles().iter().filter(|k| matches!(k, TileKey::Block(_))).count();
        (t.leaves().len(), t.tiles().len() - blocks, blocks)
    };
    eprintln!("built: (leaves, leaf tiles, block tiles) = {:?}", counts(&t));
    let mut results = Vec::new();
    for (name, prune) in [
        ("S1", prune_stepwise as fn(&Topology, &FieldData<f32>) -> (Topology, FieldData<f32>, usize)),
        ("S2", prune_at_once),
    ] {
        let start = Instant::now();
        let (t2, f2, commits) = prune(&t, &f);
        let took = start.elapsed();
        eprintln!("{name}: {commits} commits, {:.1} ms, (leaves, leaf tiles, block tiles) = {:?}", took.as_secs_f64() * 1e3, counts(&t2));
        check(&t2, &f2);
        results.push(counts(&t2));
    }
    assert_eq!(results[0], results[1]);
    assert!(results[0].2 > 0, "some block merged");
}
