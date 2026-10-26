//! Spike for B-017 (ROADMAP.md §3): what a value type defined outside brodo can do today.
//!
//! `Material`, two `u16`, takes code 99, which brodo does not assign; `Count`, an `i32`,
//! takes code 1, brodo's `f32`'s. Each goes through a field in memory, `freeze` and
//! `GridView`, and a cache file written and read back. Output on standard output; the
//! files go to the directory given.

use brodo::cache::{self, CacheEntry, CacheFile};
use brodo::error::FieldError;
use brodo::field::{Field, Value};
use brodo::frozen::freeze;
use brodo::meta::GridMeta;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::view::GridView;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct Material {
    id: u16,
    flags: u16,
}

impl Value for Material {
    const TYPE_CODE: u32 = 99;
    fn same_bits(&self, o: &Self) -> bool {
        self.id == o.id && self.flags == o.flags
    }
}

/// A user's `i32`, given the code brodo gives `f32`.
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(transparent)]
struct Count(i32);

impl Value for Count {
    const TYPE_CODE: u32 = 1;
    fn same_bits(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("a directory"));
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).expect("activate");
    let (t, _) = e.commit().expect("commit");
    let meta = GridMeta::new("m", [1.0; 3], "mm").expect("meta");

    let mut f = Field::new(&t, Material { id: 0, flags: 0 });
    par_update(&t, &mut f, |_, v| {
        v.fill(Material { id: 7, flags: 1 });
        Ok::<(), FieldError>(())
    })
    .expect("update");
    println!("Material in memory: {:?}", f.value_at(&t, [1, 2, 3]));
    let g = freeze(&t, &f).expect("freeze");
    println!("Material frozen and viewed: {:?}", GridView::<Material>::new(g.as_bytes()).map(|v| v.get([1, 2, 3])));
    let path = dir.join("material.cache");
    println!("Material cache written: {:?}", cache::write(&path, &[CacheEntry::new(&meta, &t, &f).expect("entry")]));
    println!("Material cache read by the same build: {:?}", CacheFile::read(&path, u64::MAX).map(|c| c.grid_count()));

    let mut c = Field::new(&t, Count(0));
    par_update(&t, &mut c, |_, v| {
        v.fill(Count(1_065_353_216));
        Ok::<(), FieldError>(())
    })
    .expect("update");
    let path = dir.join("count.cache");
    cache::write(&path, &[CacheEntry::new(&meta, &t, &c).expect("entry")]).expect("write");
    let file = CacheFile::read(&path, u64::MAX).expect("read");
    println!("Count 1065353216 read as f32: {:?}", file.view::<f32>(0).map(|v| v.get([1, 2, 3])));
}
