//! An archive's quantised leaf values (docs/formats.md §8.5, ADR-0016): each scalar as an
//! IEEE 754 binary16, byte-shuffled, or as 8 bits over its component's range across the
//! grid's leaf values. This module reads untrusted bytes, a chunk's codes and a grid's
//! ranges: it is listed in `UNTRUSTED_INPUT` and indexes nothing (CODING.md Rule 3).

use std::sync::OnceLock;

use zerocopy::FromBytes;
use zerocopy::little_endian::{F32, F64};

use crate::field::Stored;

/// The scalar a floating value type is made of.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Scalar {
    F32,
    F64,
}

/// A floating value type, the only kind quantised: its scalar and how many a value holds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Kind {
    scalar: Scalar,
    components: usize,
}

impl Kind {
    /// The kind of the value type of code `value_type`, if it is a floating one.
    pub(crate) fn of(value_type: u32) -> Option<Self> {
        let (scalar, components) = match value_type {
            t if t == f32::TYPE_CODE => (Scalar::F32, 1),
            t if t == f64::TYPE_CODE => (Scalar::F64, 1),
            t if t == <[f32; 3]>::TYPE_CODE => (Scalar::F32, 3),
            t if t == <[f64; 3]>::TYPE_CODE => (Scalar::F64, 3),
            _ => return None,
        };
        Some(Self { scalar, components })
    }

    /// The components a value holds, each with a range under 8 bits.
    pub(crate) fn components(self) -> usize {
        self.components
    }

    fn scalar_size(self) -> usize {
        match self.scalar {
            Scalar::F32 => 4,
            Scalar::F64 => 8,
        }
    }
}

/// A component's least and greatest value over a grid's leaf values.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Range {
    lo: f64,
    hi: f64,
}

/// Bytes a range: two `f64`.
pub(crate) const RANGE: usize = 16;

/// How a grid's leaf chunks hold its values, other than exact.
#[derive(Debug, Clone)]
pub(crate) enum Quantised {
    Binary16,
    EightBit(Vec<Range>),
}

impl Quantised {
    /// The length a leaf chunk of `raw` bytes of the frozen buffer encodes to.
    pub(crate) fn encoded_len(&self, kind: Kind, raw: usize) -> Option<usize> {
        let size = kind.scalar_size();
        let scalars = raw.is_multiple_of(size).then_some(raw / size)?;
        match self {
            Self::Binary16 => scalars.checked_mul(2),
            Self::EightBit(_) => Some(scalars),
        }
    }
}

/// Calls `f` on each scalar of `bytes`, values of `kind`, in order; false if `bytes` are
/// not whole scalars.
fn each(kind: Kind, bytes: &[u8], mut f: impl FnMut(f64)) -> bool {
    match kind.scalar {
        Scalar::F32 => <[F32]>::ref_from_bytes(bytes)
            .map(|xs| xs.iter().for_each(|x| f(f64::from(x.get()))))
            .is_ok(),
        Scalar::F64 => {
            <[F64]>::ref_from_bytes(bytes).map(|xs| xs.iter().for_each(|x| f(x.get()))).is_ok()
        }
    }
}

/// Each component's range over the leaf chunks `pieces`, values of `kind`: the first least
/// and greatest in order, so that the same bytes give the same ranges, signed zeros too.
/// `None` if a scalar is a NaN or an infinity, or a range is wider than an `f64` holds.
pub(crate) fn ranges<'a>(kind: Kind, pieces: impl Iterator<Item = &'a [u8]>) -> Option<Vec<Range>> {
    let mut r = vec![Range { lo: f64::INFINITY, hi: f64::NEG_INFINITY }; kind.components];
    let mut finite = true;
    for piece in pieces {
        let mut k = 0;
        let whole = each(kind, piece, |x| {
            finite &= x.is_finite();
            let at = k % kind.components;
            k += 1;
            if let Some(c) = r.get_mut(at) {
                if x < c.lo {
                    c.lo = x;
                }
                if x > c.hi {
                    c.hi = x;
                }
            }
        });
        finite &= whole;
    }
    for c in &mut r {
        if c.lo > c.hi {
            *c = Range { lo: 0.0, hi: 0.0 };
        }
    }
    (finite && r.iter().all(|c| (c.hi - c.lo).is_finite())).then_some(r)
}

/// The ranges' bytes, as they follow a chunk table.
pub(crate) fn range_bytes(r: &[Range]) -> Vec<u8> {
    r.iter().flat_map(|c| [c.lo.to_le_bytes(), c.hi.to_le_bytes()]).flatten().collect()
}

/// The ranges of `components` components in `bytes`, each finite, ordered and of a finite
/// width; `None` otherwise, or if `bytes` hold other than that many.
pub(crate) fn read_ranges(bytes: &[u8], components: usize) -> Option<Vec<Range>> {
    let xs = <[F64]>::ref_from_bytes(bytes).ok()?;
    if xs.len() != components.checked_mul(2)? {
        return None;
    }
    let r: Vec<_> =
        xs.as_chunks::<2>().0.iter().map(|[lo, hi]| Range { lo: lo.get(), hi: hi.get() }).collect();
    let valid = |c: &Range| c.lo.is_finite() && c.lo <= c.hi && (c.hi - c.lo).is_finite();
    r.iter().all(valid).then_some(r)
}

/// The leaf chunk `piece`, values of `kind`, encoded; `None` if a scalar is one the
/// encoding cannot hold.
pub(crate) fn encode(q: &Quantised, kind: Kind, piece: &[u8]) -> Option<Vec<u8>> {
    let mut held = true;
    match q {
        Quantised::Binary16 => {
            let mut codes = Vec::new();
            let whole = each(kind, piece, |x| match to_binary16(x) {
                Some(h) => codes.push(h),
                None => held = false,
            });
            // Shuffled: every code's low byte, then every code's high byte.
            let [low, high] =
                [0, 1].map(|b| codes.iter().map(move |h| h.to_le_bytes().get(b).copied()));
            let out: Option<Vec<u8>> = low.chain(high).collect();
            out.filter(|_| whole && held)
        }
        Quantised::EightBit(r) => {
            let mut out = Vec::new();
            let mut at = r.iter().cycle();
            let whole = each(kind, piece, |x| match at.next() {
                Some(c) if x.is_finite() => out.push(to_eight_bits(x, *c)),
                Some(_) | None => held = false,
            });
            (whole && held).then_some(out)
        }
    }
}

/// `x` within `c` as 8 bits: (x − lo) / (hi − lo) × 255 rounded half away from zero, 0
/// where the range is one value.
fn to_eight_bits(x: f64, c: Range) -> u8 {
    let width = c.hi - c.lo;
    if width == 0.0 {
        return 0;
    }
    let q = ((x - c.lo) / width * 255.0).round().clamp(0.0, 255.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "q is clamped to 0..=255 and whole"
    )]
    let q = q as u8;
    q
}

/// The largest finite binary16.
const BINARY16_MAX: f64 = 65504.0;

/// `x` as the IEEE 754 binary16 nearest it, ties to even, as its code; `None` for a NaN or
/// a value beyond ±65504.
fn to_binary16(x: f64) -> Option<u16> {
    let a = x.abs();
    if a.is_nan() || a > BINARY16_MAX {
        return None;
    }
    let sign = if x.is_sign_negative() { 0x8000 } else { 0 };
    // The binade's exponent, the subnormals sharing the least normal one, -14.
    let e = if a < 2f64.powi(-14) {
        -14
    } else {
        i32::try_from((a.to_bits() >> 52) & 0x7ff).ok()? - 1023
    };
    let r = (a / 2f64.powi(e - 10)).round_ties_even();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a is below 2^(e + 1), so r is whole and in 0..=2048"
    )]
    let r = r as u16;
    // (e + 14) << 10 plus r: a subnormal for e = -14 and r < 1024, and r = 2048 carries
    // into the next binade; at most 65504's code, a being at most 65504.
    Some(sign | ((u16::try_from(e + 14).ok()? << 10) + r))
}

/// The value of binary16 code `h`, which an `f32` holds exactly.
fn from_binary16(h: u16) -> f32 {
    let (e, m) = (i32::from(h >> 10 & 31), f32::from(h & 1023));
    let a = match e {
        0 => m * 2f32.powi(-24),
        31 if m == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1024.0 + m) * 2f32.powi(e - 25),
    };
    if h & 0x8000 == 0 { a } else { -a }
}

/// Every binary16 code's value, made once a process.
fn binary16_values() -> &'static [f32] {
    static VALUES: OnceLock<Vec<f32>> = OnceLock::new();
    VALUES.get_or_init(|| (0..=u16::MAX).map(from_binary16).collect())
}

/// Restores the leaf chunk `codes`, encoded by `q`, into `room`, values of `kind`; false
/// if their lengths disagree.
pub(crate) fn restore(q: &Quantised, kind: Kind, codes: &[u8], room: &mut [u8]) -> bool {
    match q {
        Quantised::Binary16 => {
            let values = binary16_values();
            let Some((low, high)) = codes.split_at_checked(codes.len() / 2) else {
                return false;
            };
            let value = |(l, h): (&u8, &u8)| {
                values.get(usize::from(u16::from_le_bytes([*l, *h]))).copied().unwrap_or(f32::NAN)
            };
            let pairs = low.iter().zip(high);
            match kind.scalar {
                Scalar::F32 => fill(room, low.len(), pairs.map(value).map(F32::new)),
                Scalar::F64 => {
                    fill(room, low.len(), pairs.map(value).map(|x| F64::new(f64::from(x))))
                }
            }
        }
        Quantised::EightBit(r) => {
            let steps: Vec<[f64; 256]> = r.iter().map(|c| steps(*c)).collect();
            let mut at = steps.iter().cycle();
            let mut value = move |q: &u8| {
                at.next().and_then(|s| s.get(usize::from(*q))).copied().unwrap_or(f64::NAN)
            };
            match kind.scalar {
                Scalar::F32 => {
                    fill(room, codes.len(), codes.iter().map(|q| F32::new(narrow(value(q)))))
                }
                Scalar::F64 => fill(room, codes.len(), codes.iter().map(|q| F64::new(value(q)))),
            }
        }
    }
}

/// Each 8-bit code's value over `c`: lo + q × ((hi − lo) / 255).
fn steps(c: Range) -> [f64; 256] {
    let mut s = [0.0; 256];
    for (q, v) in (0..=u8::MAX).zip(&mut s) {
        *v = c.lo + f64::from(q) * ((c.hi - c.lo) / 255.0);
    }
    s
}

/// `x` rounded to the nearest `f32`.
fn narrow(x: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rounding to the nearest f32 is the encoding's"
    )]
    let y = x as f32;
    y
}

/// Writes `n` scalars from `xs` over `room`; false unless `room` holds exactly `n`.
fn fill<T: FromBytes + zerocopy::IntoBytes + zerocopy::KnownLayout + zerocopy::Unaligned>(
    room: &mut [u8],
    n: usize,
    xs: impl Iterator<Item = T>,
) -> bool {
    match <[T]>::mut_from_bytes(room) {
        Ok(out) if out.len() == n => {
            out.iter_mut().zip(xs).for_each(|(o, x)| *o = x);
            true
        }
        Ok(_) | Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Binary16 rounds to nearest, ties to even, at a subnormal, a normal and the top of
    /// the range, and refuses beyond it.
    #[test]
    fn binary16_rounds_to_nearest_ties_to_even() {
        assert_eq!(to_binary16(1.0), Some(0x3c00));
        assert_eq!(to_binary16(-2.0), Some(0xc000));
        assert_eq!(to_binary16(-0.0), Some(0x8000));
        // 1 + 2⁻¹¹ is halfway between 1 and 1 + 2⁻¹⁰: to the even, 1.
        assert_eq!(to_binary16(1.0 + 2f64.powi(-11)), Some(0x3c00));
        assert_eq!(to_binary16(1.0 + 3.0 * 2f64.powi(-11)), Some(0x3c02));
        assert_eq!(to_binary16(2f64.powi(-24)), Some(1));
        assert_eq!(to_binary16(2f64.powi(-25)), Some(0), "a tie to the even zero");
        assert_eq!(to_binary16(65504.0), Some(0x7bff));
        assert_eq!(to_binary16(65504.5), None);
        assert_eq!(to_binary16(f64::NAN), None);
        for h in (0..0x7c00).step_by(97).chain([0x3ff, 0x400, 0x7bff]) {
            assert_eq!(to_binary16(f64::from(from_binary16(h))), Some(h), "{h:#06x}");
        }
    }

    /// Binary16 codes read as their values: the infinities, a NaN, and negatives by the
    /// sign bit, zero too.
    #[test]
    fn binary16_codes_read_as_their_values() {
        assert_eq!(from_binary16(0x7c00).to_bits(), f32::INFINITY.to_bits());
        assert_eq!(from_binary16(0xfc00).to_bits(), f32::NEG_INFINITY.to_bits());
        assert!(from_binary16(0x7e00).is_nan());
        assert!(from_binary16(0x7c01).is_nan());
        assert_eq!(from_binary16(0xc000).to_bits(), (-2.0_f32).to_bits());
        assert_eq!(from_binary16(0x8000).to_bits(), (-0.0_f32).to_bits());
    }

    /// The four floating value types have a kind, of their scalar and components; no other
    /// type has one.
    #[test]
    fn the_floating_types_alone_have_a_kind() {
        for (code, size, components) in [
            (f32::TYPE_CODE, 4, 1),
            (f64::TYPE_CODE, 8, 1),
            (<[f32; 3]>::TYPE_CODE, 4, 3),
            (<[f64; 3]>::TYPE_CODE, 8, 3),
        ] {
            let k = Kind::of(code).unwrap();
            assert_eq!((k.scalar_size(), k.components()), (size, components), "type {code}");
        }
        assert!(Kind::of(u8::TYPE_CODE).is_none());
    }

    fn scalars(xs: &[f64]) -> Vec<u8> {
        xs.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A range keeps the first least and greatest in order, so that +0 after -0 leaves it
    /// at -0 and -0 after +0 at +0; a NaN, an infinity and a piece of other than whole
    /// scalars are refused.
    #[test]
    fn a_range_keeps_the_first_extremes_and_refuses_what_is_not_finite() {
        let kind = Kind { scalar: Scalar::F64, components: 1 };
        let bits = |bytes: &[u8]| {
            ranges(kind, [bytes].into_iter())
                .map(|r| r.iter().map(|c| [c.lo.to_bits(), c.hi.to_bits()]).collect::<Vec<_>>())
        };
        let zero = |x: f64| Some(vec![[x.to_bits(); 2]]);
        assert_eq!(bits(&scalars(&[-0.0, 0.0])), zero(-0.0));
        assert_eq!(bits(&scalars(&[0.0, -0.0])), zero(0.0));
        assert_eq!(bits(&scalars(&[1.0, f64::NAN, 2.0])), None);
        assert_eq!(bits(&scalars(&[1.0, f64::INFINITY])), None);
        assert_eq!(bits(&scalars(&[1.0, 2.0])[..9]), None);
    }

    /// Ranges read are of a finite width: from -f64::MAX to f64::MAX is refused, and a
    /// range of one value at f64::MAX is not.
    #[test]
    fn ranges_read_are_of_a_finite_width() {
        assert!(read_ranges(&scalars(&[-f64::MAX, f64::MAX]), 1).is_none());
        assert!(read_ranges(&scalars(&[f64::MAX, f64::MAX]), 1).is_some());
        assert!(read_ranges(&scalars(&[-1.0, 2.0]), 1).is_some());
    }

    /// Eight bits refuse a scalar that is not finite and a piece of other than whole
    /// scalars.
    #[test]
    fn eight_bits_refuse_what_they_cannot_hold() {
        let kind = Kind { scalar: Scalar::F64, components: 1 };
        let q = Quantised::EightBit(vec![Range { lo: -1.0, hi: 1.0 }]);
        assert!(encode(&q, kind, &scalars(&[0.5, 1.0])).is_some());
        assert!(encode(&q, kind, &scalars(&[0.5, f64::NAN])).is_none());
        assert!(encode(&q, kind, &scalars(&[f64::INFINITY])).is_none());
        assert!(encode(&q, kind, &scalars(&[0.5, 1.0])[..9]).is_none());
    }

    /// A chunk restores only into room for exactly its scalars.
    #[test]
    fn a_chunk_restores_only_into_room_for_its_scalars() {
        for (scalar, size) in [(Scalar::F32, 4), (Scalar::F64, 8)] {
            let kind = Kind { scalar, components: 1 };
            for q in [Quantised::Binary16, Quantised::EightBit(vec![Range { lo: 0.0, hi: 1.0 }])] {
                let codes = encode(&q, kind, &vec![0; 2 * size]).unwrap();
                assert!(restore(&q, kind, &codes, &mut vec![0; 2 * size]), "{q:?}");
                assert!(!restore(&q, kind, &codes, &mut vec![0; 3 * size]), "{q:?}");
                assert!(!restore(&q, kind, &codes, &mut vec![0; size]), "{q:?}");
            }
        }
    }

    /// Eight bits restore within a 510th of the range, the ends exactly.
    #[test]
    fn eight_bits_restore_within_half_a_step() {
        let kind = Kind { scalar: Scalar::F64, components: 1 };
        let xs = [-3.0_f64, 0.1, 2.0, 5.0];
        let bytes: Vec<u8> = xs.iter().flat_map(|x| x.to_le_bytes()).collect();
        let r = ranges(kind, [bytes.as_slice()].into_iter()).unwrap();
        let q = Quantised::EightBit(r);
        let codes = encode(&q, kind, &bytes).unwrap();
        assert_eq!(codes.first(), Some(&0));
        assert_eq!(codes.last(), Some(&255));
        let mut room = vec![0; bytes.len()];
        assert!(restore(&q, kind, &codes, &mut room));
        for (x, y) in xs.iter().zip(room.as_chunks::<8>().0) {
            let y = f64::from_le_bytes(*y);
            assert!((x - y).abs() <= 8.0 / 510.0 * (1.0 + 1e-12), "{x} came back as {y}");
        }
    }
}
