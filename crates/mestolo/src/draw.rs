//! `slice`: a plane of a scalar grid drawn as PNG by brodo's checking output
//! (ARCHITECTURE.md §6.1), across the grid's bounds in the plane's two other axes.

use std::path::Path;

use brodo::field::Field;
use brodo::meta::{Bounds, SDF_KEY};
use brodo::slice::{Axis, ColorMap, Image, Slice, render};

use crate::error::MestoloError;
use crate::info::{Parts, extremes};
use crate::load::{Grid, Values};

/// The most pixels a slice is drawn with: 2²⁶, a 256 MiB image of four bytes a pixel.
pub const MOST_PIXELS: i64 = 1 << 26;

/// Writes grid `g`'s plane perpendicular to `axis` at `at` as PNG at `path`.
pub fn slice(g: &Grid, axis: Axis, at: i32, path: &Path) -> Result<(), MestoloError> {
    let image = match &g.values {
        Values::F32(f) => draw(g, f, axis, at)?,
        Values::F64(f) => draw(g, f, axis, at)?,
        Values::U8(f) => draw(g, f, axis, at)?,
        Values::U32(f) => draw(g, f, axis, at)?,
        Values::F32x3(_) | Values::F64x3(_) => {
            return Err(MestoloError::NotScalar(g.values.type_name()));
        }
    };
    Ok(brodo::slice::write(path, &image)?)
}

fn draw<T>(g: &Grid, f: &Field<T>, axis: Axis, at: i32) -> Result<Image, MestoloError>
where
    T: Parts + Into<f64>,
    T::Part: Into<f64>,
{
    let Bounds::Box { min: [x0, y0, z0], max: [x1, y1, z1] } = Bounds::of(&g.topology)? else {
        return Err(MestoloError::EmptyGrid(g.meta.name.clone()));
    };
    let (min, max) = match axis {
        Axis::X => ([y0, z0], [y1, z1]),
        Axis::Y => ([x0, z0], [x1, z1]),
        Axis::Z => ([x0, y0], [x1, y1]),
    };
    size(min, max)?;
    Ok(render(&g.topology, f, Slice::new(axis, at, min, max), colors(g, f)?)?)
}

/// Refuses a slice from `min` to `max`, inclusive, of more than `MOST_PIXELS`.
fn size(min: [i32; 2], max: [i32; 2]) -> Result<(), MestoloError> {
    let [[u0, v0], [u1, v1]] = [min, max];
    let width = i64::from(u1) - i64::from(u0) + 1;
    let height = i64::from(v1) - i64::from(v0) + 1;
    if width.saturating_mul(height) > MOST_PIXELS {
        return Err(MestoloError::ImageTooLarge { width, height, most: MOST_PIXELS });
    }
    Ok(())
}

/// A level set's colours diverge at zero, saturated at its background's magnitude, the
/// band's half-width; another grid's run from its least value to its greatest. A grid is a
/// level set by brodo's `SDF_KEY` or OpenVDB's `class` of `level set`
/// (docs/formats.md §9).
fn colors<T>(g: &Grid, f: &Field<T>) -> Result<ColorMap, MestoloError>
where
    T: Parts + Into<f64>,
    T::Part: Into<f64>,
{
    let extra = &g.meta.extra;
    let level_set =
        extra.contains_key(SDF_KEY) || extra.get("class").is_some_and(|c| c == "level set");
    let (least, most) = extremes(&g.topology, f)?;
    let first = |xs: Vec<Option<T::Part>>| xs.into_iter().flatten().next().map(Into::into);
    Ok(color_map(level_set, f.background().into(), first(least), first(most)))
}

/// The colour map of a grid, a level set or not, of `background` and of the least and
/// greatest of its values, none where every one is a NaN.
fn color_map(level_set: bool, background: f64, least: Option<f64>, most: Option<f64>) -> ColorMap {
    let band = background.abs();
    if level_set && band.is_finite() && band > 0.0 {
        return ColorMap::Diverging { limit: band };
    }
    let (lo, hi) = (least.unwrap_or(0.0), most.unwrap_or(0.0));
    // One value, or none but NaN, draws as the middle of a range of one around it.
    if lo < hi {
        ColorMap::Sequential { min: lo, max: hi }
    } else {
        ColorMap::Sequential { min: lo - 0.5, max: lo + 0.5 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slice_of_the_most_pixels_is_drawn_and_one_more_row_refused() {
        // 2¹³ × 2¹³ pixels is 2²⁶, from −4096 to 4095 on each side.
        assert!(size([-4096, -4096], [4095, 4095]).is_ok());
        assert!(matches!(
            size([-4096, -4096], [4095, 4096]),
            Err(MestoloError::ImageTooLarge { width: 8192, height: 8193, most: MOST_PIXELS })
        ));
        assert!(matches!(
            size([0, -8192], [8191, 0]),
            Err(MestoloError::ImageTooLarge { width: 8192, height: 8193, .. })
        ));
    }

    #[test]
    fn a_level_set_diverges_at_its_band_and_another_grid_runs_over_its_values() {
        let seq = |min, max| ColorMap::Sequential { min, max };
        assert_eq!(
            color_map(true, -3.0, Some(-3.0), Some(3.0)),
            ColorMap::Diverging { limit: 3.0 }
        );
        assert_eq!(color_map(false, 3.0, Some(-1.0), Some(2.0)), seq(-1.0, 2.0));
        // A level set with no band to saturate at takes the sequential map.
        assert_eq!(color_map(true, 0.0, Some(-1.0), Some(2.0)), seq(-1.0, 2.0));
        assert_eq!(color_map(true, f64::INFINITY, Some(-1.0), Some(2.0)), seq(-1.0, 2.0));
        assert_eq!(color_map(false, 0.0, Some(4.0), Some(4.0)), seq(3.5, 4.5));
        assert_eq!(color_map(false, 0.0, None, None), seq(-0.5, 0.5));
    }
}
