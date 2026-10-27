//! Leaf-parallel work (ARCHITECTURE.md §4.4, ADR-0015): an update that writes one field a
//! leaf at a time on `rayon`'s threads, and a reduction whose result does not depend on the
//! thread count.

use rayon::prelude::*;

use crate::error::FieldError;
use crate::field::{Field, LEAF_VOXELS, Value};
use crate::topology::{LeafNumber, Topology};

/// Writes every leaf of `out` in parallel, calling `f` with the leaf's number and its 512
/// values. `f` reads other fields by `&`, each checked against `t` (`Field::leaf`), and may
/// fail. Every leaf is visited; the error returned is that of the lowest-numbered leaf that
/// failed, whatever the thread count. A failure undoes nothing: the leaves `f` wrote, the
/// failed ones as far as it got among them, keep what it wrote.
pub fn par_update<U, E, F>(t: &Topology, out: &mut Field<U>, f: F) -> Result<(), E>
where
    U: Value,
    E: Send + From<FieldError>,
    F: Fn(LeafNumber, &mut [U; LEAF_VOXELS]) -> Result<(), E> + Sync,
{
    let numbers: Vec<LeafNumber> = t.leaf_numbers().collect();
    let leaves = out.leaves_mut(t)?;
    let results: Vec<Result<(), E>> =
        leaves.par_iter_mut().zip(numbers.par_iter()).map(|(values, n)| f(*n, values)).collect();
    results.into_iter().collect()
}

/// Reduces the leaf values of `field` to one result: `map` turns each leaf into a partial
/// result, in parallel, then `combine` folds the partials from `identity` in leaf order. The
/// result does not depend on the thread count. Tiles and the background are not included.
pub fn par_reduce<T, A, M, C>(
    t: &Topology,
    field: &Field<T>,
    identity: A,
    map: M,
    combine: C,
) -> Result<A, FieldError>
where
    T: Value,
    A: Send,
    M: Fn(LeafNumber, &[T; LEAF_VOXELS]) -> A + Sync,
    C: Fn(A, A) -> A,
{
    let numbers: Vec<LeafNumber> = t.leaf_numbers().collect();
    let leaves = field.leaves(t)?;
    let partials: Vec<A> =
        leaves.par_iter().zip(numbers.par_iter()).map(|(values, n)| map(*n, values)).collect();
    Ok(partials.into_iter().fold(identity, combine))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gather::{GATHER_VOXELS, gather};
    use crate::topology::TopologyEditor;

    /// Diffusion number of the explicit step: a power of two, so the step by hand is exact.
    const D: f32 = 0.125;

    fn topology(edit: impl FnOnce(&mut TopologyEditor<'_>)) -> Topology {
        let empty = Topology::new();
        let mut e = empty.edit();
        edit(&mut e);
        e.commit().unwrap().0
    }

    /// One explicit step of the heat equation with the 7-point Laplacian, through a gather.
    fn step(t: &Topology, old: &Field<f32>, new: &mut Field<f32>) -> Result<(), FieldError> {
        par_update(t, new, |n, out| {
            let mut g = [0.0; GATHER_VOXELS];
            gather(t, old, n, &mut g)?;
            for (i, o) in out.iter_mut().enumerate() {
                let c = (i % 8 + 1) + 10 * (i / 8 % 8 + 1) + 100 * (i / 64 + 1);
                let lap = g[c - 1] + g[c + 1] + g[c - 10] + g[c + 10] + g[c - 100] + g[c + 100];
                *o = g[c] + D * (lap - 6.0 * g[c]);
            }
            Ok(())
        })
    }

    #[test]
    fn one_diffusion_step_matches_the_step_by_hand() {
        let t = topology(|e| {
            e.activate([0, 0, 0]).unwrap();
        });
        let n = t.leaf_numbers().next().unwrap();
        let mut old = Field::new(&t, 0.0_f32);
        old.leaf_mut(&t, n).unwrap()[4 + 8 * 4 + 64 * 4] = 1.0;
        let mut new = Field::new(&t, 0.0_f32);
        step(&t, &old, &mut new).unwrap();
        for (i, v) in new.leaf(&t, n).unwrap().iter().enumerate() {
            let p = [i % 8, i / 8 % 8, i / 64].map(|c| c.abs_diff(4));
            let want: f32 = match p.iter().sum::<usize>() {
                0 => 0.25,
                1 => 0.125,
                _ => 0.0,
            };
            assert_eq!(v.to_bits(), want.to_bits(), "voxel {i}");
        }
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            // Magnitudes from 1e-3 to 1e5, so that sums round differently in another order.
            let m = (self.0 >> 40) as f32 / (1 << 24) as f32;
            m * 10_f32.powi(i32::try_from((self.0 >> 33) % 9).unwrap() - 3)
        }
    }

    fn cluster() -> (Topology, Field<f32>) {
        let t = topology(|e| {
            for i in 0..300 {
                e.activate([i % 10 * 8, i / 10 % 6 * 8, i / 60 * 8]).unwrap();
            }
            e.add_tile(crate::tree::TileLevel::Lower, [80, 0, 0]).unwrap();
        });
        let mut f = Field::new(&t, 0.5_f32);
        let mut g = Lcg(11);
        for n in t.leaf_numbers().collect::<Vec<_>>() {
            for v in f.leaf_mut(&t, n).unwrap() {
                *v = g.next();
            }
        }
        (t, f)
    }

    fn in_pool<R: Send>(threads: usize, f: impl FnOnce() -> R + Send) -> R {
        rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap().install(f)
    }

    #[test]
    fn same_bits_at_any_thread_count() {
        let (t, old) = cluster();
        let sum = |f: &Field<f32>| par_reduce(&t, f, 0.0_f32, |_, v| v.iter().sum(), |a, b| a + b);
        let run = |threads| {
            in_pool(threads, || {
                let mut new = Field::new(&t, 0.5_f32);
                step(&t, &old, &mut new).unwrap();
                let bits: Vec<u32> =
                    new.leaves(&t).unwrap().iter().flatten().map(|v| v.to_bits()).collect();
                (bits, sum(&new).unwrap().to_bits(), sum(&old).unwrap().to_bits())
            })
        };
        let one = run(1);
        // The sum in leaf order, worked out here one leaf after another.
        let mut want = 0.0_f32;
        for n in t.leaf_numbers() {
            want += old.leaf(&t, n).unwrap().iter().sum::<f32>();
        }
        assert_eq!(one.2, want.to_bits());
        for threads in [2, 8] {
            assert_eq!(run(threads), one, "{threads} threads");
        }
    }

    #[test]
    fn the_lowest_failing_leaf_is_reported() {
        let (t, _) = cluster();
        let mut out = Field::new(&t, 0_u8);
        for threads in [1, 2, 8] {
            let got = in_pool(threads, || {
                par_update(&t, &mut out, |n, _| {
                    if n.get() % 7 == 3 { Err(FieldError::UnknownLeaf(n.get())) } else { Ok(()) }
                })
            });
            assert_eq!(got, Err(FieldError::UnknownLeaf(3)), "{threads} threads");
        }
    }

    #[test]
    fn a_stale_field_is_refused() {
        let (t, f) = cluster();
        let (t2, _) = t.edit().commit().unwrap();
        let mut out = f.clone();
        let r: Result<(), FieldError> = par_update(&t2, &mut out, |_, _| Ok(()));
        assert_eq!(r, Err(FieldError::StaleField));
        assert_eq!(par_reduce(&t2, &f, 0, |_, _| 1, |a, b| a + b), Err(FieldError::StaleField));
    }
}
