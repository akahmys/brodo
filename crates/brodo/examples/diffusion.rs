//! Diffusion of two fields over one topology, double-buffered, with the leaf-parallel update
//! and the gather (ROADMAP.md, P1 slice).
//!
//! Textbook material only (AGENTS.md S1): the heat equation u_t = D ∇²u, its explicit
//! forward-time centred-space scheme with the 7-point Laplacian on a unit grid,
//! u ← u + D Δt ∇²u, stable for D Δt ≤ 1/6, and its Gaussian solution
//! u(r, t) = (σ0² / σ²)^(3/2) exp(−r² / 2σ²) with σ² = σ0² + 2 D t.
//!
//! Two fields diffuse at once, temperature and concentration, with different D. The region
//! grows as the Gaussians widen, so each field is remapped at each commit. The checks:
//! the error against the Gaussian falls as the square of the grid step between two
//! resolutions, and the sum of the values is kept.

use std::error::Error;

use brodo::error::FieldError;
use brodo::field::{Field, uniform};
use brodo::gather::{GATHER_VOXELS, gather};
use brodo::parallel::{par_reduce, par_update};
use brodo::topology::Topology;
use brodo::tree::Coord;

type BoxError = Box<dyn Error + Send + Sync>;

/// The time step, in units where the grid step is 1; with D ≤ 1, D Δt ≤ 1/8.
const DT: f32 = 0.125;
/// The two fields' diffusivities: temperature, concentration.
const DIFFUSIVITY: [f32; 2] = [1.0, 0.5];
/// The region covers the Gaussian to this many standard deviations, where it is below
/// exp(−18), about 1.5e-8.
const COVER: f32 = 6.0;

/// One field, double-buffered.
struct Species {
    d: f32,
    now: Field<f32>,
    next: Field<f32>,
}

struct Solver {
    topology: Topology,
    species: Vec<Species>,
    /// Leaves cover voxels from −radius to radius − 1 on each axis.
    radius: i32,
}

/// A problem: the Gaussian's width at the start, in voxels, and the steps to run.
#[derive(Clone, Copy)]
pub struct Resolution {
    sigma0: f32,
    steps: u32,
}

/// The coarse grid: temperature's width grows from 2 to 3 voxels, as σ² = 4 + 2 · 20 Δt.
pub const COARSE: Resolution = Resolution { sigma0: 2.0, steps: 20 };
/// The fine grid halves the step: from 4 to 6 voxels, as σ² = 16 + 2 · 80 Δt.
pub const FINE: Resolution = Resolution { sigma0: 4.0, steps: 80 };

/// What a run ends with, per field.
pub struct Outcome {
    /// The largest difference from the Gaussian over every leaf voxel.
    pub error: [f32; 2],
    /// The sum of the values at the start and at the end.
    pub sums: [(f32, f32); 2],
}

fn gaussian(v: Coord, sigma0: f32, sigma_sq: f32) -> f32 {
    let r_sq: f32 = v.iter().map(|c| (*c as f32) * (*c as f32)).sum();
    (sigma0 * sigma0 / sigma_sq).powf(1.5) * (-r_sq / (2.0 * sigma_sq)).exp()
}

/// The radius, a whole number of leaves, covering `COVER` standard deviations.
#[expect(clippy::cast_possible_truncation, reason = "a radius of a few dozen voxels")]
fn radius_for(sigma: f32) -> i32 {
    8 * (COVER * sigma / 8.0).ceil() as i32
}

impl Solver {
    fn new(r: Resolution) -> Result<Self, BoxError> {
        let mut s = Solver { topology: Topology::new(), species: Vec::new(), radius: 0 };
        s.grow(radius_for(r.sigma0))?;
        for d in DIFFUSIVITY {
            let mut now = Field::new(&s.topology, 0.0);
            let t = &s.topology;
            par_update(t, &mut now, |n, out| -> Result<(), BoxError> {
                for (voxel, u) in t.leaf_voxels(n)?.zip(out.iter_mut()) {
                    *u = gaussian(voxel, r.sigma0, r.sigma0 * r.sigma0);
                }
                Ok(())
            })?;
            let next = Field::new(&s.topology, 0.0);
            s.species.push(Species { d, now, next });
        }
        Ok(s)
    }

    /// Activates every leaf out to `radius` and remaps every field, both buffers.
    fn grow(&mut self, radius: i32) -> Result<(), BoxError> {
        if radius <= self.radius {
            return Ok(());
        }
        let mut e = self.topology.edit();
        for z in (-radius..radius).step_by(8) {
            for y in (-radius..radius).step_by(8) {
                for x in (-radius..radius).step_by(8) {
                    e.activate([x, y, z])?;
                }
            }
        }
        let (topology, remap) = e.commit()?;
        for s in &mut self.species {
            s.now = s.now.remap(&remap, &uniform)?;
            s.next = s.next.remap(&remap, &uniform)?;
        }
        self.topology = topology;
        self.radius = radius;
        Ok(())
    }

    /// One explicit step of every field.
    fn step(&mut self) -> Result<(), FieldError> {
        let t = &self.topology;
        for s in &mut self.species {
            let (now, lambda) = (&s.now, s.d * DT);
            par_update(t, &mut s.next, |n, out| {
                let mut g = [0.0; GATHER_VOXELS];
                gather(t, now, n, &mut g)?;
                for (i, u) in out.iter_mut().enumerate() {
                    let c = (i % 8 + 1) + 10 * (i / 8 % 8 + 1) + 100 * (i / 64 + 1);
                    let sum = g[c - 1] + g[c + 1] + g[c - 10] + g[c + 10] + g[c - 100] + g[c + 100];
                    *u = g[c] + lambda * (sum - 6.0 * g[c]);
                }
                Ok(())
            })?;
            std::mem::swap(&mut s.now, &mut s.next);
        }
        Ok(())
    }

    fn sum(&self, k: usize) -> Result<f32, FieldError> {
        par_reduce(&self.topology, &self.species[k].now, 0.0, |_, v| v.iter().sum(), |a, b| a + b)
    }

    /// The largest difference from the Gaussian of field `k` at time `t`.
    fn error(&self, k: usize, r: Resolution, t: f32) -> Result<f32, BoxError> {
        let sigma_sq = r.sigma0 * r.sigma0 + 2.0 * self.species[k].d * t;
        let topology = &self.topology;
        par_reduce(
            topology,
            &self.species[k].now,
            Ok(0.0_f32),
            |n, values| -> Result<f32, BoxError> {
                let diff = |(v, u): (Coord, &f32)| (u - gaussian(v, r.sigma0, sigma_sq)).abs();
                Ok(topology.leaf_voxels(n)?.zip(values).map(diff).fold(0.0, f32::max))
            },
            |a, b| Ok(a?.max(b?)),
        )?
    }
}

/// Runs a problem for its steps.
pub fn run(r: Resolution) -> Result<Outcome, BoxError> {
    let mut solver = Solver::new(r)?;
    let start = [solver.sum(0)?, solver.sum(1)?];
    let steps = r.steps;
    for k in 1..=steps {
        let sigma = (r.sigma0 * r.sigma0 + 2.0 * k as f32 * DT).sqrt();
        solver.grow(radius_for(sigma))?;
        solver.step()?;
    }
    let t = steps as f32 * DT;
    Ok(Outcome {
        error: [solver.error(0, r, t)?, solver.error(1, r, t)?],
        sums: [(start[0], solver.sum(0)?), (start[1], solver.sum(1)?)],
    })
}

fn main() -> Result<(), BoxError> {
    let coarse = run(COARSE)?;
    let fine = run(FINE)?;
    check(&coarse, &fine)
}

/// Largest error allowed against the Gaussian on the fine grid (TESTING.md T4). Measured on
/// 2026-10-17: 4.27e-4 (temperature) and 1.02e-3 (concentration).
const ERROR_TOL: f32 = 1.5e-3;
/// The range the coarse-to-fine error ratio must fall in, 4 for second order. Measured on
/// 2026-10-17: 3.93 and 4.20.
const RATIO: std::ops::RangeInclusive<f32> = 3.5..=4.5;
/// Largest relative change of a field's sum: what leaves the region is below exp(−18), so
/// rounding is all that remains. Measured on 2026-10-17: at most 1.3e-6.
const SUM_TOL: f32 = 1e-5;

fn check(coarse: &Outcome, fine: &Outcome) -> Result<(), BoxError> {
    for k in 0..2 {
        if fine.error[k] > ERROR_TOL {
            return Err(format!("field {k}: error {} above {ERROR_TOL}", fine.error[k]).into());
        }
        let ratio = coarse.error[k] / fine.error[k];
        if !RATIO.contains(&ratio) {
            return Err(format!("field {k}: error ratio {ratio} outside {RATIO:?}").into());
        }
        for o in [coarse, fine] {
            let (a, b) = o.sums[k];
            if ((b - a) / a).abs() > SUM_TOL {
                return Err(format!("field {k}: sum {a} became {b}").into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gaussian_is_followed_to_second_order_and_its_sum_kept() {
        let coarse = run(COARSE).unwrap();
        let fine = run(FINE).unwrap();
        check(&coarse, &fine).unwrap();
    }
}
