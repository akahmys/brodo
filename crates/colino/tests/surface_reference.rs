//! Reference tier (TESTING.md §1) for the input's surface (ADR-0028): known closed
//! surfaces give V − E + F and are closed 2-manifolds; each way of breaking one is counted
//! as its own; and spheres with faces turned over or taken away are counted as a search,
//! edge by edge and fan by fan, written another way, counts them.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use colino::error::MeshError;
use colino::mesh::Mesh;
use colino::surface::Surface;
use proptest::prelude::*;

mod common;
use common::{sphere, torus};

fn tetrahedron() -> Mesh {
    Mesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    )
    .with_unit(None)
}

#[test]
fn closed_surfaces_count_their_euler_characteristic() {
    for (name, mesh, chi) in [
        ("tetrahedron", tetrahedron(), 2),
        ("sphere", sphere(2, 1.0, [0.0; 3]).welded().unwrap(), 2),
        ("torus", torus(3.0, 1.0, 12), 0),
    ] {
        let s = Surface::of(&mesh).unwrap();
        assert!(s.closed_manifold(), "{name}: {s:?}");
        assert_eq!(s.euler(), chi, "{name}");
    }
    let t = Surface::of(&tetrahedron()).unwrap();
    assert_eq!([t.vertices, t.edges, t.faces], [4, 6, 4]);
}

#[test]
fn each_way_of_breaking_a_surface_is_counted() {
    let count = |m: &Mesh| Surface::of(m).unwrap();
    let mut open = tetrahedron();
    open.triangles.pop();
    let s = count(&open);
    assert_eq!((s.open_edges, s.euler(), s.closed_manifold()), (3, 1, false));

    let mut flipped = tetrahedron();
    flipped.triangles[0].swap(1, 2);
    assert_eq!(count(&flipped).misoriented_edges, 3);

    let mut crowded = tetrahedron();
    crowded.vertices.push([1.0, 1.0, 1.0]);
    crowded.triangles.push([0, 1, 4]);
    let s = count(&crowded);
    assert_eq!((s.crowded_edges, s.open_edges), (1, 2));

    // Two tetrahedra touching at vertex 0.
    let mut pinched = tetrahedron();
    pinched.vertices.extend([[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]]);
    pinched.triangles.extend([[0, 4, 5], [0, 6, 4], [0, 5, 6], [4, 6, 5]]);
    let s = count(&pinched);
    assert_eq!(
        (s.fanless_vertices, s.euler(), s.open_edges, s.closed_manifold()),
        (1, 3, 0, false)
    );

    let mut degenerate = tetrahedron();
    // A corner repeated in each of its three places.
    degenerate.triangles.extend([[0, 0, 1], [1, 0, 0], [0, 1, 0]]);
    let s = count(&degenerate);
    assert_eq!((s.degenerate_triangles, s.faces, s.euler()), (3, 4, 2));

    let mut bad = tetrahedron();
    bad.triangles.push([0, 1, 9]);
    assert_eq!(Surface::of(&bad), Err(MeshError::BadIndex { triangle: 4, index: 9 }));
}

#[test]
fn an_unwelded_soup_is_a_closed_surface_once_welded() {
    let t = tetrahedron();
    let soup = Mesh::new(
        t.triangles.iter().flat_map(|f| f.map(|i| t.vertices[i as usize])).collect(),
        (0..4).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect(),
    )
    .with_unit(None);
    assert_eq!(Surface::of(&soup).unwrap().open_edges, 12);
    assert!(Surface::of(&soup.welded().unwrap()).unwrap().closed_manifold());
}

/// The counts by another way: each edge's faces gathered and their directions compared;
/// each vertex's faces searched breadth first through edges at the vertex.
fn by_search(m: &Mesh) -> [u64; 3] {
    let mut faces_of: BTreeMap<(u32, u32), Vec<bool>> = BTreeMap::new();
    for [a, b, c] in &m.triangles {
        for (u, v) in [(*a, *b), (*b, *c), (*c, *a)] {
            faces_of.entry((u.min(v), u.max(v))).or_default().push(u < v);
        }
    }
    let open = faces_of.values().filter(|f| f.len() == 1).count() as u64;
    let misoriented = faces_of.values().filter(|f| f.len() == 2 && f[0] == f[1]).count() as u64;
    let mut fanless = 0;
    let vertices: BTreeSet<u32> = m.triangles.iter().flatten().copied().collect();
    for v in vertices {
        let mine: Vec<&[u32; 3]> = m.triangles.iter().filter(|t| t.contains(&v)).collect();
        let shares = |a: &[u32; 3], b: &[u32; 3]| a.iter().any(|x| *x != v && b.contains(x));
        let mut seen = vec![false; mine.len()];
        let mut queue = VecDeque::from([0]);
        seen[0] = true;
        while let Some(i) = queue.pop_front() {
            for j in 0..mine.len() {
                if !seen[j] && shares(mine[i], mine[j]) {
                    seen[j] = true;
                    queue.push_back(j);
                }
            }
        }
        fanless += u64::from(seen.iter().any(|s| !s));
    }
    [open, misoriented, fanless]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn broken_spheres_count_as_a_search_does(
        // How often a face is turned over, and kept: none, a few or many breaks of each.
        flip_rate in prop_oneof![Just(0.0), Just(0.02), Just(0.5)],
        keep_rate in prop_oneof![Just(1.0), Just(0.97), Just(0.85)],
        draws in prop::collection::vec([0.0..1.0_f64, 0.0..1.0], 128),
    ) {
        let flips: Vec<bool> = draws.iter().map(|d| d[0] < flip_rate).collect();
        let keep: Vec<bool> = draws.iter().map(|d| d[1] < keep_rate).collect();
        // The common sphere lists each triangle's midpoints apart: welded, they are one.
        let mut m = sphere(2, 1.0, [0.0; 3]).welded().unwrap();
        let mut triangles = Vec::new();
        for ((t, flip), kept) in m.triangles.iter().zip(&flips).zip(&keep) {
            if *kept {
                let mut t = *t;
                if *flip {
                    t.swap(1, 2);
                }
                triangles.push(t);
            }
        }
        prop_assume!(!triangles.is_empty());
        m.triangles = triangles;
        let s = Surface::of(&m).unwrap();
        prop_assert_eq!([s.open_edges, s.misoriented_edges, s.fanless_vertices], by_search(&m));
        prop_assert_eq!(s.closed_manifold(), by_search(&m) == [0, 0, 0] && s.crowded_edges == 0);
    }
}
