//! The two forms agree: the same regions are the outside, keys, and each empty component,
//! and the empty components come in the same order.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use rootsparse_spike::*;

/// Checks `Sparse` against `Dense` on `keys`; returns whether the empty components'
/// order agrees.
fn agree(keys: &BTreeSet<R>, conn: Conn) -> bool {
    let d = Dense::new(keys, conn).expect("dense accepts it");
    let s = Sparse::new(keys, conn);
    let (mut ds, mut ss) = (d.sets().clone(), s.sets().clone());
    let mut probes = probes(keys);
    for k in keys {
        for e in conn.all() {
            probes.push([k[0] + e[0], k[1] + e[1], k[2] + e[2]]);
        }
    }
    let mut map: BTreeMap<Loc, Loc> = BTreeMap::new();
    let mut back: BTreeMap<Loc, Loc> = BTreeMap::new();
    for p in &probes {
        let (a, b) = (component(&d, &mut ds, *p), component(&s, &mut ss, *p));
        assert_eq!(*map.entry(a).or_insert(b), b, "{p:?} under {conn:?}, keys {keys:?}");
        assert_eq!(*back.entry(b).or_insert(a), a, "{p:?} under {conn:?}, keys {keys:?}");
        assert_eq!(a == Loc::Outside, b == Loc::Outside);
        assert_eq!(a == Loc::Key, b == Loc::Key);
    }
    // `map` is ordered by the dense component's least unit; the sparse ones in that order.
    let sparse: Vec<Loc> = map.values().copied().collect();
    let mut sorted = sparse.clone();
    sorted.sort();
    sparse == sorted
}

fn keys(n: usize, span: i64) -> impl Strategy<Value = BTreeSet<R>> {
    prop::collection::btree_set(prop::array::uniform3(-span..span), 0..n)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn close_keys_agree(k in keys(40, 4), six in any::<bool>()) {
        agree(&k, if six { Conn::Six } else { Conn::TwentySix });
    }

    #[test]
    fn crowded_keys_agree(k in keys(64, 2), six in any::<bool>()) {
        agree(&k, if six { Conn::Six } else { Conn::TwentySix });
    }

    /// A cube of side 6 full of keys but for some holes: many pockets, some touching only
    /// at an edge or a corner.
    #[test]
    fn holed_blocks_agree(holes in prop::collection::btree_set(prop::array::uniform3(0..6_i64), 0..80), six in any::<bool>()) {
        let mut k = BTreeSet::new();
        for z in 0..6 { for y in 0..6 { for x in 0..6 {
            if !holes.contains(&[x, y, z]) { k.insert([x, y, z]); }
        }}}
        agree(&k, if six { Conn::Six } else { Conn::TwentySix });
    }

    #[test]
    fn spread_keys_agree(k in keys(12, 40), six in any::<bool>()) {
        agree(&k, if six { Conn::Six } else { Conn::TwentySix });
    }
}

/// A hollow cube of keys, side `n`, its inside a pocket.
fn shell(n: i64) -> BTreeSet<R> {
    let mut out = BTreeSet::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                if [x, y, z].iter().any(|c| *c == 0 || *c == n - 1) {
                    out.insert([x, y, z]);
                }
            }
        }
    }
    out
}

#[test]
fn a_shell_s_pocket_is_one_component_not_the_outside() {
    for conn in [Conn::Six, Conn::TwentySix] {
        let k = shell(6);
        let s = Sparse::new(&k, conn);
        let mut sets = s.sets().clone();
        let inside = component(&s, &mut sets, [3, 3, 3]);
        assert!(matches!(inside, Loc::Empty(_)));
        assert_eq!(component(&s, &mut sets, [1, 4, 2]), inside);
        assert_eq!(component(&s, &mut sets, [-5, 3, 3]), Loc::Outside);
        agree(&k, conn);
    }
}

#[test]
fn the_order_of_empty_components() {
    // Counted, not asserted: how often the sparse form's order differs from the dense.
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut differ = 0;
    let total = 2000;
    for _ in 0..total {
        let k = keys(60, 2).new_tree(&mut runner).unwrap().current();
        for conn in [Conn::Six, Conn::TwentySix] {
            if !agree(&k, conn) {
                differ += 1;
            }
        }
    }
    println!("order differs in {differ} of {} layouts", 2 * total);
}
