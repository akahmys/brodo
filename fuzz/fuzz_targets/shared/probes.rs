//! Voxels around every level's boundaries, read from any grid or region a target accepts.

/// The voxels every target reads.
pub const PROBES: [[i32; 3]; 8] = [
    [0, 0, 0],
    [-1, -1, -1],
    [7, 8, 9],
    [127, 128, 129],
    [4095, 0, 4096],
    [-4097, 5, 300],
    [i32::MAX, 0, i32::MIN],
    [64, 64, 64],
];
