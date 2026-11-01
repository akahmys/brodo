//! Reads one `.vdb` file written by `gen.py` and checks it holds the leaves and tiles that
//! file was made to hold, so that a measurement of nothing stops the run.
//!
//! Usage: `vdbmemory-spike <file> <leaves> <tiles>`

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, path, leaves, tiles] = args.as_slice() else {
        panic!("usage: vdbmemory-spike <file> <leaves> <tiles>");
    };
    let file = brodo::vdb::read(std::path::Path::new(path), "m", u64::MAX).unwrap();
    let t = &file.grids[0].topology;
    assert_eq!(t.leaf_numbers().count(), leaves.parse::<usize>().unwrap());
    assert_eq!(t.tile_numbers().count(), tiles.parse::<usize>().unwrap());
}
