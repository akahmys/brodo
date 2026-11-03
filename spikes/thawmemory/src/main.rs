//! Writes a cache file that makes the most when thawed, for its kind: upper-level tiles, nodes
//! each holding one tile, or leaves; and prints its counts of leaves and tiles.
//!
//! Usage: `thawmemory-spike tiles|nodes|leaves <n> <out>`
use brodo::cache::{CacheEntry, write};
use brodo::field::Field;
use brodo::meta::GridMeta;
use brodo::topology::Topology;
use brodo::tree::TileLevel;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (kind, n, out): (&str, i32, &str) = (&a[1], a[2].parse().unwrap(), &a[3]);
    let empty = Topology::new();
    let mut e = empty.edit();
    match kind {
        // n upper nodes, each with 32767 upper-level tiles
        "tiles" => for u in 0..n { for p in 0..32767 {
            e.add_tile(TileLevel::Upper, [u * 4096 + p % 32 * 128, p / 32 % 32 * 128, p / 1024 * 128]).unwrap(); } },
        // n upper nodes, each with one lower node holding one lower-level tile
        "nodes" => for u in 0..n { e.add_tile(TileLevel::Lower, [u * 4096, 0, 0]).unwrap(); },
        // n leaves
        _ => for k in 0..n { e.activate([k % 512 * 8, k / 512 % 512 * 8, k / 262144 * 8]).unwrap(); },
    }
    let t = e.commit().unwrap().0;
    let f = Field::new(&t, 1.0_f32);
    let m = GridMeta::new("g", [1.0; 3], "m").unwrap();
    write(std::path::Path::new(out), &[CacheEntry::new(&m, &t, &f).unwrap()]).unwrap();
    println!("{} leaves {} tiles", t.leaf_count(), t.tile_count());
}
