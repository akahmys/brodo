//! A box of voxels taken from the input, so that the input chooses what a region read
//! covers.

use brodo::meta::Bounds;

/// A box from the input's last 12 bytes: a corner, each axis three bytes, signed, within
/// ±2²³, and a size an axis, up to 255 voxels; none from an input shorter.
pub fn extent_of(data: &[u8]) -> Option<Bounds> {
    let tail = data.get(data.len().checked_sub(12)?..)?;
    let byte = |i: usize| tail.get(i).copied().unwrap_or(0);
    let axis = |k: usize| (i32::from_le_bytes([byte(3 * k), byte(3 * k + 1), byte(3 * k + 2), 0]) << 8) >> 8;
    let min = [axis(0), axis(1), axis(2)];
    let max = [0, 1, 2].map(|k| min[k] + i32::from(byte(9 + k)));
    Some(Bounds::Box { min, max })
}
