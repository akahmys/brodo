//! Checking output (ARCHITECTURE.md §6.1, ADR-0018): a slice of a scalar field drawn as an
//! image and encoded as PNG, for a person or an agent to look at (TESTING.md T8).
//!
//! One voxel is one pixel. A leaf voxel takes its value's colour; a tile's voxel its
//! value's colour with every fourth diagonal darkened; the background a flat grey, since it
//! holds no stored value; a value that is not a number magenta. The colour range is the
//! caller's, never inferred, so the same input always gives the same image and the same
//! PNG bytes.

use crate::accessor::ValueAccessor;
use crate::error::SliceError;
use crate::field::{Field, Value};
use crate::topology::{Location, Topology};
use crate::tree::Coord;

/// The axis a slice is perpendicular to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// The slice is the plane x = position; the image's u is y, its v is z.
    X,
    /// The plane y = position; u is x, v is z.
    Y,
    /// The plane z = position; u is x, v is y.
    Z,
}

/// Where to cut and what to draw: the plane, and the inclusive range of its two other
/// coordinates (u, v). The image's top row is the largest v.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    /// The axis the plane is perpendicular to.
    pub axis: Axis,
    /// The plane's coordinate along that axis.
    pub position: i32,
    /// The smallest u and v drawn.
    pub min: [i32; 2],
    /// The largest u and v drawn.
    pub max: [i32; 2],
}

impl Slice {
    fn voxel(&self, u: i32, v: i32) -> Coord {
        match self.axis {
            Axis::X => [self.position, u, v],
            Axis::Y => [u, self.position, v],
            Axis::Z => [u, v, self.position],
        }
    }
}

/// How values become colours; the range is always given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorMap {
    /// For signed distances: blue below zero, white at zero, red above, saturated at
    /// ±`limit`; the zero contour drawn in black.
    Diverging {
        /// The magnitude drawn at full colour.
        limit: f64,
    },
    /// For other scalars: dark at `min`, light at `max`, clamped beyond.
    Sequential {
        /// The value drawn darkest.
        min: f64,
        /// The value drawn lightest.
        max: f64,
    },
}

impl ColorMap {
    /// Whether the range draws anything: a finite positive `limit`, or finite `min` and
    /// `max` with `min` below `max`. Any other range divides by zero or by a NaN and paints
    /// in no colour of its own.
    fn check(self) -> Result<(), SliceError> {
        let usable = match self {
            ColorMap::Diverging { limit } => limit.is_finite() && limit > 0.0,
            ColorMap::Sequential { min, max } => min.is_finite() && max.is_finite() && min < max,
        };
        if usable { Ok(()) } else { Err(SliceError::BadColorMap(self)) }
    }
}

/// An image: rows from the top, four bytes per pixel (red, green, blue, alpha).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Pixels per row.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

const BACKGROUND: [u8; 3] = [128, 128, 128];
const NOT_A_NUMBER: [u8; 3] = [255, 0, 255];
const CONTOUR: [u8; 3] = [0, 0, 0];
const BLUE: [f64; 3] = [59.0, 76.0, 192.0];
const WHITE: [f64; 3] = [247.0, 247.0, 247.0];
const RED: [f64; 3] = [180.0, 4.0, 38.0];
const DARK: [f64; 3] = [68.0, 1.0, 84.0];
const MIDDLE: [f64; 3] = [33.0, 145.0, 140.0];
const LIGHT: [f64; 3] = [253.0, 231.0, 37.0];

/// The colour a fraction `t` of the way from `a` to `b`, rounded.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "rounded within 0..=255"
)]
fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [u8; 3] {
    [0, 1, 2].map(|k| (a[k] + (b[k] - a[k]) * t).round().clamp(0.0, 255.0) as u8)
}

fn color(map: ColorMap, v: f64) -> [u8; 3] {
    if v.is_nan() {
        return NOT_A_NUMBER;
    }
    match map {
        ColorMap::Diverging { limit } => {
            let t = (v / limit).clamp(-1.0, 1.0);
            if t < 0.0 { lerp(WHITE, BLUE, -t) } else { lerp(WHITE, RED, t) }
        }
        ColorMap::Sequential { min, max } => {
            let t = ((v - min) / (max - min)).clamp(0.0, 1.0);
            if t < 0.5 { lerp(DARK, MIDDLE, 2.0 * t) } else { lerp(MIDDLE, LIGHT, 2.0 * t - 1.0) }
        }
    }
}

/// What a pixel shows before the contour is drawn.
#[derive(Debug, Clone, Copy)]
enum Pixel {
    Value(f64),
    Tile(f64),
    Background,
}

/// Draws a slice of `f`; `T` is a scalar, read as `f64`.
pub fn render<T: Value + Into<f64>>(
    t: &Topology,
    f: &Field<T>,
    slice: Slice,
    map: ColorMap,
) -> Result<Image, SliceError> {
    map.check()?;
    let width = u32::try_from(i64::from(slice.max[0]) - i64::from(slice.min[0]) + 1)
        .map_err(|_| SliceError::EmptyRange)?;
    let height = u32::try_from(i64::from(slice.max[1]) - i64::from(slice.min[1]) + 1)
        .map_err(|_| SliceError::EmptyRange)?;
    if width == 0 || height == 0 {
        return Err(SliceError::EmptyRange);
    }
    let mut acc = ValueAccessor::new(t, f)?;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for v in (slice.min[1]..=slice.max[1]).rev() {
        for u in slice.min[0]..=slice.max[0] {
            let voxel = slice.voxel(u, v);
            pixels.push(match acc.locate(voxel) {
                Location::Leaf(_) => Pixel::Value(acc.get(voxel).into()),
                Location::Tile(..) => Pixel::Tile(acc.get(voxel).into()),
                Location::Background => Pixel::Background,
            });
        }
    }
    let w = width as usize;
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for (i, p) in pixels.iter().enumerate() {
        let (row, col) = (i / w, i % w);
        let rgb = match *p {
            Pixel::Background => BACKGROUND,
            Pixel::Value(_) if on_contour(map, &pixels, i, w) => CONTOUR,
            Pixel::Value(x) => color(map, x),
            Pixel::Tile(x) if (row + col).is_multiple_of(4) => color(map, x).map(|c| c / 2 + c / 8),
            Pixel::Tile(x) => color(map, x),
        };
        rgba.extend_from_slice(&rgb);
        rgba.push(255);
    }
    Ok(Image { width, height, rgba })
}

/// Whether a pixel lies on the zero contour of a diverging map: its value and a neighbour's
/// (left, right, up or down) differ in sign, and its value is the one nearer zero, so the
/// line is one pixel wide and unbroken.
fn on_contour(map: ColorMap, pixels: &[Pixel], i: usize, w: usize) -> bool {
    let (ColorMap::Diverging { .. }, Some(Pixel::Value(x))) = (map, pixels.get(i)) else {
        return false;
    };
    let left =
        if i.is_multiple_of(w) { None } else { i.checked_sub(1).and_then(|j| pixels.get(j)) };
    let right = if (i + 1).is_multiple_of(w) { None } else { pixels.get(i + 1) };
    let up = i.checked_sub(w).and_then(|j| pixels.get(j));
    [left, right, up, pixels.get(i + w)].into_iter().flatten().any(|n| match *n {
        Pixel::Value(y) | Pixel::Tile(y) => (*x < 0.0) != (y < 0.0) && x.abs() <= y.abs(),
        Pixel::Background => false,
    })
}

/// Encodes an image as PNG: 8-bit RGBA, with fixed compression and filtering, so that the
/// same image always gives the same bytes.
pub fn encode_png(image: &Image) -> Result<Vec<u8>, SliceError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Balanced);
    encoder.set_filter(png::Filter::Adaptive);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image.rgba)?;
    writer.finish()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(map: ColorMap) -> Result<Image, SliceError> {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.activate([0, 0, 0]).unwrap();
        let t = e.commit().unwrap().0;
        let f = Field::new(&t, 1.0_f32);
        render(&t, &f, Slice { axis: Axis::Z, position: 0, min: [0, 0], max: [7, 7] }, map)
    }

    #[test]
    fn a_colour_map_with_no_usable_range_is_refused() {
        for map in [
            ColorMap::Diverging { limit: 0.0 },
            ColorMap::Diverging { limit: -1.0 },
            ColorMap::Diverging { limit: f64::NAN },
            ColorMap::Diverging { limit: f64::INFINITY },
            ColorMap::Sequential { min: 1.0, max: 1.0 },
            ColorMap::Sequential { min: 2.0, max: 1.0 },
            ColorMap::Sequential { min: f64::NAN, max: 1.0 },
            ColorMap::Sequential { min: 0.0, max: f64::INFINITY },
        ] {
            // By text, since a NaN is not equal to itself.
            let refused = match draw(map) {
                Err(SliceError::BadColorMap(m)) => format!("{m:?}"),
                other => format!("{other:?}"),
            };
            assert_eq!(refused, format!("{map:?}"));
        }
    }

    #[test]
    fn a_colour_map_with_a_range_draws() {
        for map in
            [ColorMap::Diverging { limit: 3.0 }, ColorMap::Sequential { min: -1.0, max: 1.0 }]
        {
            assert_eq!(draw(map).unwrap().rgba.len(), 8 * 8 * 4, "{map:?}");
        }
    }

    #[test]
    fn a_range_empty_along_one_axis_is_refused() {
        let t = Topology::new();
        let f = Field::new(&t, 1.0_f32);
        let map = ColorMap::Diverging { limit: 3.0 };
        for (min, max) in [([0, 0], [-1, 7]), ([0, 0], [7, -1])] {
            let got = render(&t, &f, Slice { axis: Axis::Z, position: 0, min, max }, map);
            assert!(matches!(got, Err(SliceError::EmptyRange)), "{max:?}: {got:?}");
        }
    }

    /// Two neighbours of equal size and opposite sign are both on the contour: neither is
    /// nearer zero, so leaving either out would break the line.
    #[test]
    fn equal_and_opposite_neighbours_are_both_on_the_contour() {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.activate([0, 0, 0]).unwrap();
        let t = e.commit().unwrap().0;
        let mut f = Field::new(&t, 2.0_f32);
        let n = t.leaf_numbers().next().unwrap();
        let leaf = f.leaf_mut(&t, n).unwrap();
        leaf.fill(2.0);
        (leaf[0], leaf[1]) = (0.5, -0.5);
        let slice = Slice { axis: Axis::Z, position: 0, min: [0, 0], max: [7, 7] };
        let image = render(&t, &f, slice, ColorMap::Diverging { limit: 3.0 }).unwrap();
        // Rows run from the top, so y = 0 is the last of eight.
        let rgb = |x: usize| &image.rgba[(7 * 8 + x) * 4..(7 * 8 + x) * 4 + 3];
        assert_eq!((rgb(0), rgb(1)), (&CONTOUR[..], &CONTOUR[..]));
        assert_ne!(rgb(2), &CONTOUR[..]);
    }

    /// A sequential map runs from dark at `min` through the middle colour to light at
    /// `max`, wherever the range lies.
    #[test]
    fn a_sequential_map_spans_its_range() {
        let map = ColorMap::Sequential { min: 10.0, max: 30.0 };
        let at = |c: [f64; 3]| lerp(c, c, 0.0);
        let dark = at(DARK);
        assert_eq!(color(map, 10.0), dark);
        assert_eq!(color(map, 5.0), dark);
        assert_eq!(color(map, 20.0), at(MIDDLE));
        assert_eq!(color(map, 30.0), at(LIGHT));
        assert_eq!(color(map, 25.0), lerp(MIDDLE, LIGHT, 0.5));
    }

    /// Zeros beside positive values, and beside each other, are not on the contour: zero
    /// counts with the positive side, so no sign changes there.
    #[test]
    fn zeros_among_positives_are_off_the_contour() {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.activate([0, 0, 0]).unwrap();
        let t = e.commit().unwrap().0;
        let mut f = Field::new(&t, 2.0_f32);
        let n = t.leaf_numbers().next().unwrap();
        let leaf = f.leaf_mut(&t, n).unwrap();
        leaf.fill(2.0);
        (leaf[0], leaf[1]) = (0.0, 0.0);
        let slice = Slice { axis: Axis::Z, position: 0, min: [0, 0], max: [7, 7] };
        let image = render(&t, &f, slice, ColorMap::Diverging { limit: 3.0 }).unwrap();
        for x in [0, 1] {
            let at = (7 * 8 + x) * 4;
            assert_eq!(&image.rgba[at..at + 3], &lerp(WHITE, WHITE, 0.0)[..], "pixel {x}");
        }
    }
}
