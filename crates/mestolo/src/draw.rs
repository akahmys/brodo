//! `slice`: a plane of a scalar grid, or of a component of a vector grid, drawn as PNG by
//! brodo's checking output (ARCHITECTURE.md §6.1, §6.3), across the grid's bounds in the
//! plane's two other axes or the region given, in the colours of the range given or else
//! of its values.

use std::path::Path;

use brodo::field::{AnyField, Field, Value};
use brodo::meta::{Bounds, SDF_KEY};
use brodo::slice::{Axis, ColorMap, Image, Slice, render};
use brodo::topology::Topology;

use crate::args::{Component, Drawing};

use crate::error::MestoloError;
use crate::info::{Parts, extremes};
use crate::load::Grid;

/// The most pixels a slice is drawn with: 2²⁶, a 256 MiB image of four bytes a pixel.
pub const MOST_PIXELS: i64 = 1 << 26;

/// Writes grid `g`'s plane perpendicular to `axis` at `at` as PNG at `path`, as `drawing`
/// says, refusing a plane beyond the grid's leaves and tiles along `axis`, a vector grid
/// with no component named and a scalar grid with one.
pub fn slice(
    g: &Grid,
    axis: Axis,
    at: i32,
    drawing: &Drawing,
    path: &Path,
) -> Result<(), MestoloError> {
    let level_set = level_set(g);
    let t = &g.topology;
    let image = match (&g.field, drawing.component) {
        (AnyField::F32(f), None) => draw(g, f, axis, at, drawing, level_set)?,
        (AnyField::F64(f), None) => draw(g, f, axis, at, drawing, level_set)?,
        (AnyField::U8(f), None) => draw(g, f, axis, at, drawing, level_set)?,
        (AnyField::U32(f), None) => draw(g, f, axis, at, drawing, level_set)?,
        (AnyField::F32x3(f), Some(c)) => draw(g, &component(t, f, c)?, axis, at, drawing, false)?,
        (AnyField::F64x3(f), Some(c)) => draw(g, &component(t, f, c)?, axis, at, drawing, false)?,
        (AnyField::F32x3(_) | AnyField::F64x3(_), None) => {
            return Err(MestoloError::NotScalar(g.field.value_type()));
        }
        (AnyField::F32(_) | AnyField::F64(_) | AnyField::U8(_) | AnyField::U32(_), Some(_)) => {
            return Err(MestoloError::ComponentOfScalar(g.field.value_type()));
        }
        _ => return Err(MestoloError::ValueType(g.field.value_type())),
    };
    Ok(brodo::slice::write(path, &image)?)
}

/// The field of component `c` of `f`'s values, its leaves', its tiles' and its
/// background's, as `f64`: a magnitude is found without overflowing on the way.
fn component<T>(t: &Topology, f: &Field<[T; 3]>, c: Component) -> Result<Field<f64>, MestoloError>
where
    T: Copy + Into<f64>,
    [T; 3]: Value,
{
    let pick = |v: [T; 3]| {
        let [x, y, z] = v.map(Into::into);
        match c {
            Component::X => x,
            Component::Y => y,
            Component::Z => z,
            Component::Magnitude => x.hypot(y).hypot(z),
        }
    };
    let mut out = Field::new(t, pick(f.background()));
    for n in t.leaf_numbers() {
        let values = f.leaf(t, n)?;
        for (o, v) in out.leaf_mut(t, n)?.iter_mut().zip(values) {
            *o = pick(*v);
        }
    }
    for n in t.tile_numbers() {
        out.set_tile(t, n, pick(f.tile(t, n)?))?;
    }
    Ok(out)
}

fn draw<T>(
    g: &Grid,
    f: &Field<T>,
    axis: Axis,
    at: i32,
    drawing: &Drawing,
    level_set: bool,
) -> Result<Image, MestoloError>
where
    T: Parts + Into<f64>,
    T::Part: Into<f64>,
{
    let Bounds::Box { min: [x0, y0, z0], max: [x1, y1, z1] } = Bounds::of(&g.topology)? else {
        return Err(MestoloError::EmptyGrid(g.meta.name.clone()));
    };
    let (min, max, span) = match axis {
        Axis::X => ([y0, z0], [y1, z1], (x0, x1)),
        Axis::Y => ([x0, z0], [x1, z1], (y0, y1)),
        Axis::Z => ([x0, y0], [x1, y1], (z0, z1)),
    };
    if at < span.0 || at > span.1 {
        let grid = g.meta.name.clone();
        return Err(MestoloError::OutsideGrid { grid, axis, at, min: span.0, max: span.1 });
    }
    let [min, max] = drawing.region.unwrap_or([min, max]);
    size(min, max)?;
    let map = match drawing.range {
        Some([min, max]) => ColorMap::Sequential { min, max },
        None => colors(level_set, g, f)?,
    };
    Ok(render(&g.topology, f, Slice::new(axis, at, min, max), map)?)
}

/// Whether `g` is a level set: by brodo's `SDF_KEY` or OpenVDB's `class` of `level set`
/// (docs/formats.md §9).
fn level_set(g: &Grid) -> bool {
    let extra = &g.meta.extra;
    extra.contains_key(SDF_KEY) || extra.get("class").is_some_and(|c| c == "level set")
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
/// band's half-width; another grid's run from its least value to its greatest.
fn colors<T>(level_set: bool, g: &Grid, f: &Field<T>) -> Result<ColorMap, MestoloError>
where
    T: Parts + Into<f64>,
    T::Part: Into<f64>,
{
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
