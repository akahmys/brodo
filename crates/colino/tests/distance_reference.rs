//! Reference tier (TESTING.md §1): the distance from a point to a mesh, through the tree
//! of boxes, equals a search of every triangle by another method: the point projected to a
//! triangle's plane where the projection falls inside it, else the nearest of its three
//! edges. Triangles with no area, points on the surface and ties are among the cases.

use colino::bvh::Bvh;
use colino::distance::{Point, on_segment, on_triangle, squared};
use colino::mesh::Mesh;
use proptest::prelude::*;

/// The rounding of a distance found by two methods, relative to the distance, or to 1 for
/// a shorter one.
const RELATIVE_ROUNDING: f64 = 1e-12;
/// The rounding of a squared distance between two points of a triangle of unit size found
/// by two methods: zero but for it.
const SQUARED_ROUNDING: f64 = 1e-24;
/// The distance by which a point of a thin triangle is found off it, relative to the
/// triangle's size: √ε, the rounding of a normal no longer than √ε of its edges' product,
/// where the face's point gives way to the edges'.
const SLIVER_ROUNDING: f64 = 1.5e-8;

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Point, b: Point) -> Point {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The squared distance by projection: inside the triangle when the three sub-triangles
/// the projection makes with its edges turn the triangle's way.
fn by_projection(p: Point, [a, b, c]: [Point; 3]) -> f64 {
    let edges = [(a, b), (b, c), (c, a)];
    let to_edges =
        edges.iter().map(|(u, v)| squared(p, on_segment(p, *u, *v))).fold(f64::INFINITY, f64::min);
    let n = cross(sub(b, a), sub(c, a));
    let nn = dot(n, n);
    if nn == 0.0 {
        return to_edges;
    }
    let s = dot(sub(p, a), n) / nn;
    let q = [p[0] - s * n[0], p[1] - s * n[1], p[2] - s * n[2]];
    let inside = edges.iter().all(|(u, v)| dot(cross(sub(*v, *u), sub(q, *u)), n) >= 0.0);
    if inside { to_edges.min(squared(p, q)) } else { to_edges }
}

#[test]
fn the_nearest_point_of_a_triangle_by_region() {
    let t = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
    let cases: [(Point, Point); 7] = [
        ([0.5, 0.5, 3.0], [0.5, 0.5, 0.0]),   // the face
        ([-1.0, -1.0, 0.0], [0.0, 0.0, 0.0]), // corner a
        ([3.0, -1.0, 1.0], [2.0, 0.0, 0.0]),  // corner b
        ([-1.0, 3.0, -1.0], [0.0, 2.0, 0.0]), // corner c
        ([1.0, -2.0, 0.0], [1.0, 0.0, 0.0]),  // edge ab
        ([-2.0, 1.0, 5.0], [0.0, 1.0, 0.0]),  // edge ac
        ([2.0, 2.0, 0.0], [1.0, 1.0, 0.0]),   // edge bc
    ];
    for (p, want) in cases {
        assert_eq!(on_triangle(p, t).map(f64::to_bits), want.map(f64::to_bits), "{p:?}");
    }
    // No area: corners on one line, or one point.
    let line = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [3.0, 0.0, 0.0]];
    assert_eq!(
        on_triangle([2.0, 1.0, 0.0], line).map(f64::to_bits),
        [2.0, 0.0, 0.0].map(f64::to_bits)
    );
    // On a line off the axes, where each part of the normal is a sum of non-zero products.
    let slant = [[0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [3.0, 6.0, 9.0]];
    let (p, q) = ([2.0, 1.0, 0.0], on_triangle([2.0, 1.0, 0.0], slant));
    let foot = on_segment(p, slant[0], slant[2]);
    assert!(squared(q, foot) < SQUARED_ROUNDING, "{q:?} against {foot:?}");
    // Triangles with area whose normal has one part a difference of equal products, so
    // that a sum in its place would call them flat: points over their faces fall on them.
    for (t, p, want) in [
        ([[0.0, 0.0, 0.0], [0.0, 1.0, 1.0], [0.0, 1.0, -1.0]], [5.0, 0.7, 0.0], [0.0, 0.7, 0.0]),
        ([[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [1.0, 0.0, -1.0]], [0.7, 5.0, 0.0], [0.7, 0.0, 0.0]),
        ([[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, -1.0, 0.0]], [0.7, 0.0, 5.0], [0.7, 0.0, 0.0]),
    ] {
        assert_eq!(on_triangle(p, t).map(f64::to_bits), want.map(f64::to_bits), "{t:?}");
    }
    let point = [[1.0, 1.0, 1.0]; 3];
    assert_eq!(on_triangle([0.0, 0.0, 0.0], point).map(f64::to_bits), [1.0; 3].map(f64::to_bits));
}

#[test]
fn an_empty_mesh_has_no_nearest_triangle() {
    assert_eq!(Bvh::new(&Mesh::default()).unwrap().nearest([0.0; 3]), None);
}

/// Small integer coordinates, so that ties and triangles with no area are common.
fn coordinate() -> impl Strategy<Value = f64> {
    (-6..6_i32).prop_map(|c| f64::from(c) / 2.0)
}

fn point() -> impl Strategy<Value = Point> {
    [coordinate(), coordinate(), coordinate()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_tree_finds_what_every_triangle_searched_finds(
        vertices in prop::collection::vec(point(), 3..30),
        picks in prop::collection::vec([any::<prop::sample::Index>(), any::<prop::sample::Index>(), any::<prop::sample::Index>()], 1..60),
        queries in prop::collection::vec(prop_oneof![point(), [-8.0..8.0_f64, -8.0..8.0, -8.0..8.0]], 1..20),
    ) {
        let n = vertices.len();
        let triangles: Vec<[u32; 3]> = picks.iter().map(|t| t.map(|i| u32::try_from(i.index(n)).unwrap())).collect();
        let mesh = Mesh::new(vertices, triangles).with_unit(None);
        let tree = Bvh::new(&mesh).unwrap();
        let corners = |t: &[u32; 3]| t.map(|i| mesh.vertices[usize::try_from(i).unwrap()]);
        for q in queries {
            let (d, i) = tree.nearest(q).unwrap();
            let want = mesh.triangles.iter().map(|t| by_projection(q, corners(t))).fold(f64::INFINITY, f64::min).sqrt();
            prop_assert!((d - want).abs() <= RELATIVE_ROUNDING * (1.0 + want), "{} against {} at {:?}", d, want, q);
            // The triangle named is nearest, and of those as near, the first: compared by
            // the same arithmetic the tree uses, exactly.
            let at = |t: &[u32; 3]| squared(q, on_triangle(q, corners(t)));
            let own = at(&mesh.triangles[usize::try_from(i).unwrap()]);
            prop_assert_eq!(own.sqrt().to_bits(), d.to_bits());
            for (j, t) in mesh.triangles.iter().enumerate() {
                let order = at(t).total_cmp(&own);
                prop_assert!(order.is_gt() || (order.is_eq() && j >= usize::try_from(i).unwrap()), "triangle {} against {}", j, i);
            }
        }
    }
    /// A triangle whose corners lie on a line, but for the rounding of the third, has a
    /// normal of rounding alone: its nearest point is never farther than its edges' nearest,
    /// whatever the size of its coordinates.
    #[test]
    fn a_sliver_is_never_farther_than_its_edges(
        a in [-1.0..1.0_f64, -1.0..1.0, -1.0..1.0],
        b in [-1.0..1.0_f64, -1.0..1.0, -1.0..1.0],
        t in 0.0..1.0_f64,
        p in [-2.0..2.0_f64, -2.0..2.0, -2.0..2.0],
        size in prop::sample::select(vec![1e-6, 1.0, 1e6]),
    ) {
        let [a, b, p] = [a, b, p].map(|q| q.map(|x| x * size));
        let c = [0, 1, 2].map(|k| a[k] + t * (b[k] - a[k]));
        for tri in [[a, b, c], [a, c, b], [c, a, b]] {
            let edges = [(0, 1), (1, 2), (2, 0)]
                .map(|(i, j)| squared(p, on_segment(p, tri[i], tri[j])))
                .into_iter()
                .fold(f64::INFINITY, f64::min);
            let d = squared(p, on_triangle(p, tri));
            prop_assert!(d <= edges * (1.0 + RELATIVE_ROUNDING), "{} against {} for {:?}", d, edges, tri);
        }
    }

    /// A point on an edge of a triangle of any thinness, its third corner off the line of
    /// the others by 10⁻¹² to 10⁻² of their distance, is found on the triangle: no farther
    /// than `SLIVER_ROUNDING` of the triangle's size, whatever the size of its coordinates.
    #[test]
    fn a_point_on_a_thin_triangles_edge_is_found_on_it(
        a in [-1.0..1.0_f64, -1.0..1.0, -1.0..1.0],
        b in [-1.0..1.0_f64, -1.0..1.0, -1.0..1.0],
        off in [-1.0..1.0_f64, -1.0..1.0, -1.0..1.0],
        thinness in -12.0..-2.0_f64,
        t in 0.2..3.0_f64,
        s in 0.0..1.0_f64,
        edge in 0..3_usize,
        size in prop::sample::select(vec![1e-6, 1.0, 1e6]),
    ) {
        let width = 10_f64.powf(thinness) * dot(sub(b, a), sub(b, a)).sqrt();
        let c = [0, 1, 2].map(|k| a[k] + t * (b[k] - a[k]) + width * off[k]);
        let [a, b, c] = [a, b, c].map(|q| q.map(|x| x * size));
        let tri = [a, b, c];
        let (u, v) = (tri[edge], tri[(edge + 1) % 3]);
        let p = [0, 1, 2].map(|k| u[k] + s * (v[k] - u[k]));
        let extent = [b, c].map(|q| dot(sub(q, a), sub(q, a))).into_iter().fold(0.0, f64::max).sqrt();
        let d = squared(p, on_triangle(p, tri)).sqrt();
        prop_assert!(d <= SLIVER_ROUNDING * extent, "{} of {} for {:?} at {:?}", d, extent, tri, p);
    }
}
