#!/usr/bin/env python3
"""A reader of brodo's files written from docs/formats.md alone (ADR-0037).

    formats.py [--brief] FILE...
    formats.py --patch FILE < PATCHES

For each file it prints `file PATH`, then either `refused REASON` or what it read: the
grids, each with its metadata, background, leaves, some of their voxels found as §2.4
says, and tiles, then each Hermite set's
verdict, and `end`. With `--brief`, a read file prints `accepted` in place of its grids.
With `--patch`, each line of standard input, a byte offset and a `u32` in hexadecimal,
gives a copy of FILE with that word replaced, read as `--brief` reads, named
`FILE@OFFSET=VALUE`.
`crates/brodo/tests/format_document.rs` compares the output with what the crates read.
Section numbers below are the document's.
"""

import struct
import sys

BLOCK = 32
# §1.1: code -> (size, struct format of one value).
TYPES = {1: (4, "<f"), 2: (8, "<d"), 3: (1, "<B"), 4: (4, "<I"), 5: (12, "<3f"), 6: (24, "<3d")}
LEVEL_EDGE_LOG2 = {"root": 12, "upper": 7, "lower": 3}
UPPER_NODE, LOWER_NODE = 8224, 1056
FIELDS = ["sign"] + [f"{a}.{f}" for a in "xyz" for f in ("fraction", "normal", "group")]
FIELD_TYPES = {"sign": 3, "fraction": 1, "normal": 5, "group": 4}
NO_CROSSING = (struct.pack("<f", -1.0), struct.pack("<3f", 0.0, 0.0, 0.0), 0)


class Refused(Exception):
    pass


def need(ok, why):
    if not ok:
        raise Refused(why)


def u32(b, at):
    need(at + 4 <= len(b), "truncated")
    return struct.unpack_from("<I", b, at)[0]


def place(local, bits):
    """§1.2: bit i of x, y, z to bits 3i, 3i + 1, 3i + 2."""
    p = 0
    for i in range(bits):
        for axis in range(3):
            p |= ((local[axis] >> i) & 1) << (3 * i + axis)
    return p


def unplace(p, bits):
    out = [0, 0, 0]
    for i in range(bits):
        for axis in range(3):
            out[axis] |= ((p >> (3 * i + axis)) & 1) << i
    return out


def local(v, child_log2, bits):
    return [((c & 0xFFFFFFFF) >> child_log2) & ((1 << bits) - 1) for c in v]


def origin(v, edge_log2):
    return [c & ~((1 << edge_log2) - 1) for c in v]


def bit(mask, p):
    return (mask[p // 32] >> (p % 32)) & 1


def below(mask, p):
    n = sum(bin(w).count("1") for w in mask[: p // 32])
    return n + bin(mask[p // 32] & ((1 << (p % 32)) - 1)).count("1")


class Grid:
    """A frozen buffer, §2, checked as §2.5 says."""

    def __init__(self, b, value_type=None):
        need(len(b) % BLOCK == 0 and len(b) >= 96, "length")
        need(b[0:4] == b"BRDG", "magic")
        need(u32(b, 4) == 1, "version")
        code, size = u32(b, 8), u32(b, 12)
        need(code in TYPES and TYPES[code][0] == size, "value type")
        need(value_type is None or code == value_type, "value type")
        self.code, self.size = code, size
        counts = [u32(b, 16 + 4 * i) for i in range(5)]
        offsets = [u32(b, 36 + 4 * i) for i in range(5)]
        need(u32(b, 56) * BLOCK == len(b), "blocks")
        self.background = b[60 : 60 + size]
        entry = [32, UPPER_NODE, LOWER_NODE, 512 * size, size]
        at, sections = 96, []
        for count, offset, e in zip(counts, offsets, entry):
            start, end = offset * BLOCK, offset * BLOCK + count * e
            need(start >= at and end <= len(b), "section")
            sections.append((start, count))
            at = end
        (r, nr), (u, nu), (lo, nl), (lv, self.nleaves), (t, self.ntiles) = sections
        self.b, self.leaf_at, self.tile_at = b, lv, t
        self.tree = (b[r : r + 32 * nr], b[u : u + UPPER_NODE * nu], b[lo : lo + LOWER_NODE * nl])
        self.roots = []
        for i in range(nr):
            key = list(struct.unpack_from("<3i", b, r + 32 * i))
            kind, index = u32(b, r + 32 * i + 12), u32(b, r + 32 * i + 16)
            need(all(k % 4096 == 0 for k in key), "root key")
            need(not self.roots or self.roots[-1][0] < key, "root order")
            need(kind in (0, 1) and index < (self.ntiles if kind == 0 else nu), "root index")
            self.roots.append((key, kind, index))
        self.uppers = [self.node(u + UPPER_NODE * i, 1024, nl) for i in range(nu)]
        self.lowers = [self.node(lo + LOWER_NODE * i, 128, self.nleaves) for i in range(nl)]

    def node(self, at, words, nchildren):
        b = self.b
        children = list(struct.unpack_from(f"<{words}I", b, at))
        tiles = list(struct.unpack_from(f"<{words}I", b, at + 4 * words))
        first_child, first_tile = u32(b, at + 8 * words), u32(b, at + 8 * words + 4)
        ones = lambda m: sum(bin(w).count("1") for w in m)
        need(first_child + ones(children) <= nchildren, "node children")
        need(first_tile + ones(tiles) <= self.ntiles, "node tiles")
        need(all(c & t == 0 for c, t in zip(children, tiles)), "node masks")
        return children, tiles, first_child, first_tile

    def leaf(self, n):
        at = self.leaf_at + n * 512 * self.size
        return self.b[at : at + 512 * self.size]

    def tile(self, i):
        return self.b[self.tile_at + i * self.size : self.tile_at + (i + 1) * self.size]

    def locate(self, v):
        """§2.4: ("leaf", n), ("tile", place) or ("background",)."""
        key = origin(v, 12)
        root = next((e for e in self.roots if e[0] == key), None)
        if root is None:
            return ("background",)
        if root[1] == 0:
            return ("tile", root[2])
        children, tiles, fc, ft = self.uppers[root[2]]
        p = place(local(v, 7, 5), 5)
        if bit(tiles, p):
            return ("tile", ft + below(tiles, p))
        if not bit(children, p):
            return ("background",)
        children, tiles, fc2, ft2 = self.lowers[fc + below(children, p)]
        p = place(local(v, 3, 4), 4)
        if bit(children, p):
            return ("leaf", fc2 + below(children, p))
        if bit(tiles, p):
            return ("tile", ft2 + below(tiles, p))
        return ("background",)

    def get(self, v):
        where = self.locate(v)
        if where[0] == "leaf":
            o = origin(v, 3)
            x, y, z = (v[k] - o[k] for k in range(3))
            i = x + 8 * y + 64 * z
            return self.leaf(where[1])[i * self.size : (i + 1) * self.size]
        if where[0] == "tile":
            return self.tile(where[1])
        return self.background

    def regions(self):
        """Every leaf's origin by number, and every tile's (level, origin, place)."""
        leaves, tiles = [None] * self.nleaves, []
        for key, kind, index in self.roots:
            if kind == 0:
                tiles.append(("root", key, index))
                continue
            children, utiles, fc, ft = self.uppers[index]
            for p in range(32 ** 3):
                if not (bit(children, p) or bit(utiles, p)):
                    continue
                at = [key[k] + 128 * c for k, c in enumerate(unplace(p, 5))]
                if bit(utiles, p):
                    tiles.append(("upper", at, ft + below(utiles, p)))
                    continue
                lch, ltiles, lfc, lft = self.lowers[fc + below(children, p)]
                for q in range(16 ** 3):
                    leaf_at = [at[k] + 8 * c for k, c in enumerate(unplace(q, 4))]
                    if bit(ltiles, q):
                        tiles.append(("lower", leaf_at, lft + below(ltiles, q)))
                    if bit(lch, q):
                        leaves[lfc + below(lch, q)] = leaf_at
        return leaves, tiles


class Cursor:
    def __init__(self, b):
        self.b, self.at = b, 0

    def take(self, n):
        need(self.at + n <= len(self.b), "metadata truncated")
        out = self.b[self.at : self.at + n]
        self.at += n
        return out

    def u32(self):
        return struct.unpack("<I", self.take(4))[0]

    def string(self):
        n = self.u32()
        s = self.take(n)
        need(self.take(-n % 4) == b"\0" * (-n % 4), "string padding")
        try:
            s.decode("utf-8")
        except UnicodeDecodeError:
            raise Refused("utf-8")
        return s


def metadata(b):
    """§3.3, checked as §3.4 rule 4 says."""
    c = Cursor(b)
    m = {"name": c.string(), "unit": c.string()}
    m["voxel_size"] = [c.take(8) for _ in range(3)]
    m["transform"] = [c.take(8) for _ in range(12)]
    kind = c.u32()
    corners = list(struct.unpack("<6i", c.take(24)))
    need(kind in (0, 1), "bounds kind")
    need(kind == 1 or corners == [0] * 6, "empty bounds")
    m["bounds"] = corners if kind == 1 else None
    pairs = []
    for _ in range(c.u32()):
        pairs.append((c.string(), c.string()))
    need(all(a[0] < b[0] for a, b in zip(pairs, pairs[1:])), "key order")
    need(c.at == len(b), "metadata length")
    need(m["unit"] != b"", "unit")
    f64 = lambda x: struct.unpack("<d", x)[0]
    finite = lambda x: f64(x) - f64(x) == 0
    need(all(finite(x) and f64(x) > 0 for x in m["voxel_size"]), "voxel size")
    need(all(finite(x) for x in m["transform"]), "transform")
    m["pairs"] = pairs
    return m


def cache(b):
    """§3, checked as §3.4 says: a list of (metadata, grid)."""
    need(len(b) % BLOCK == 0, "length")
    need(len(b) >= BLOCK and b[0:4] == b"BRDC", "magic")
    need(u32(b, 4) == 1, "version")
    count = u32(b, 8)
    need(b[12:32] == b"\0" * 20, "header")
    after, out = (count + 1) * BLOCK, []
    for i in range(count):
        d = 32 * (1 + i)
        need(d + 32 <= len(b), "directory")
        meta_at, meta_len, grid_at, grid_blocks, code = (u32(b, d + 4 * k) for k in range(5))
        need(b[d + 20 : d + 32] == b"\0" * 12, "directory")
        start = meta_at * BLOCK
        need(start == after and start + meta_len <= len(b), "metadata place")
        meta_end = start + meta_len
        grid_start = -(-meta_end // BLOCK) * BLOCK
        grid_end = grid_start + grid_blocks * BLOCK
        need(grid_at * BLOCK == grid_start and grid_end <= len(b), "grid place")
        need(b[meta_end:grid_start] == b"\0" * (grid_start - meta_end), "padding")
        m = metadata(b[start:meta_end])
        need(code in TYPES, "value type")
        out.append((m, Grid(b[grid_start:grid_end], code)))
        after = grid_end
    need(after == len(b), "end")
    return out


def group_table(pairs):
    """§4.1: the names by id, group 0 first; [] when there is no group key."""
    by_id = {}
    for k, v in pairs:
        if not k.startswith(b"hermite.group."):
            continue
        id_ = k[len(b"hermite.group.") :]
        need(id_.isdigit() and id_[:1] != b"0" and int(id_) < 2 ** 32, "group key")
        by_id[int(id_)] = v
    need(sorted(by_id) == list(range(1, len(by_id) + 1)), "group ids")
    names = [v for _, v in sorted(by_id.items())]
    need(all(names) and len(set(names)) == len(names), "group names")
    return [b""] + names if names else []


def hermite(grids, name):
    """§4.2: checks the set `name`; returns its group table."""
    found = {}
    for m, g in grids:
        pairs = dict(m["pairs"])
        if pairs.get(b"hermite.set") != name:
            continue
        need(pairs.get(b"hermite.version") == b"1", "hermite version")
        field = pairs.get(b"hermite.field", b"").decode()
        need(field in FIELDS and field not in found, "hermite field")
        need(g.code == FIELD_TYPES[field.split(".")[-1]], "hermite type")
        found[field] = (m, g)
    need(len(found) == 10, "hermite missing")
    sm, s = found["sign"]
    for f in FIELDS[1:]:
        m, g = found[f]
        same = (m["voxel_size"], m["transform"], m["unit"])
        need(same == (sm["voxel_size"], sm["transform"], sm["unit"]), "hermite metadata")
        need(g.tree == s.tree, "hermite topology")
    names = group_table(sm["pairs"])
    groups = max(len(names), 1)
    axes = [tuple(found[f"{a}.{f}"][1] for f in ("fraction", "normal", "group")) for a in "xyz"]

    def crossing(axis, get):
        f, n, g = axes[axis]
        return get(f), get(n), struct.unpack("<I", get(g))[0]

    sign = lambda v: s.get(v)[0]
    need(s.background == b"\0", "sign background")
    for a in range(3):
        need(crossing(a, lambda g: g.background) == NO_CROSSING, "crossing background")
    leaves, tiles = s.regions()
    need(None not in leaves, "leaf unreached")
    for _, _, t in tiles:
        need(s.tile(t)[0] <= 1, "tile sign")
        for a in range(3):
            need(crossing(a, lambda g: g.tile(t)) == NO_CROSSING, "tile crossing")
    for n in range(s.nleaves):
        need(all(x <= 1 for x in s.leaf(n)), "leaf sign")
    is_leaf = lambda v: s.locate(v)[0] == "leaf"
    unit = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    for level, o, t in tiles:
        edge, inside = 1 << LEVEL_EDGE_LOG2[level], s.tile(t)[0]
        for a in range(3):
            e, p, q = unit[a], unit[(a + 1) % 3], unit[(a + 2) % 3]
            for i in range(0, edge, 8):
                for j in range(0, edge, 8):
                    lower = [o[k] + p[k] * i + q[k] * j for k in range(3)]
                    for v in ([lower[k] + e[k] * edge for k in range(3)],
                              [lower[k] - e[k] for k in range(3)]):
                        need(is_leaf(v) or sign(v) == inside, "tile face")
    for n, o in enumerate(leaves):
        for i in range(512):
            v = [o[0] + i % 8, o[1] + i // 8 % 8, o[2] + i // 64]
            here = sign(v)
            for a in range(3):
                w = [v[k] + unit[a][k] for k in range(3)]
                c = crossing(a, lambda g: g.leaf(n)[i * g.size : (i + 1) * g.size])
                if sign(w) == here:
                    need(c == NO_CROSSING, "crossing where signs agree")
                else:
                    fraction = struct.unpack("<f", c[0])[0]
                    normal = struct.unpack("<3f", c[1])
                    length = sum(x * x for x in normal) ** 0.5
                    need(0.0 <= fraction <= 1.0, "fraction")
                    need(abs(length - 1.0) <= 1e-6, "normal length")
                    need(normal[a] >= 0 if here == 1 else normal[a] <= 0, "normal inward")
                    need(c[2] < groups, "group")
                u = [v[k] - unit[a][k] for k in range(3)]
                need(is_leaf(u) or sign(u) == here, "owner")
    return names


def read(path, brief, b=None):
    if b is None:
        b = open(path, "rb").read()
    print("file", path)
    try:
        if b[0:4] == b"BRDG":
            grids, sets = [(None, Grid(b))], []
        else:
            grids = cache(b)
            sets = sorted({dict(m["pairs"]).get(b"hermite.set") for m, _ in grids} - {None})
        verdicts = []
        for name in sets:
            try:
                verdicts.append(f"hermite {name.hex()} ok " + " ".join(n.hex() or "-" for n in hermite(grids, name)))
            except (Refused, struct.error, IndexError, ZeroDivisionError) as e:
                verdicts.append(f"hermite {name.hex()} refused {e}")
    except (Refused, struct.error, IndexError) as e:
        print("refused", e)
        return
    if brief:
        print("accepted")
    else:
        for i, (m, g) in enumerate(grids):
            dump(i, m, g)
    for v in verdicts:
        print(v)
    print("end")


def dump(i, m, g):
    print("grid", i, g.code)
    if m is not None:
        print("name", m["name"].hex() or "-")
        print("unit", m["unit"].hex())
        print("voxel_size", *(x.hex() for x in m["voxel_size"]))
        print("transform", *(x.hex() for x in m["transform"]))
        print("bounds", *(m["bounds"] if m["bounds"] else ["empty"]))
        for k, v in m["pairs"]:
            print("pair", k.hex(), v.hex() or "-")
    print("background", g.background.hex())
    leaves, tiles = g.regions()
    for n, o in enumerate(leaves):
        print("leaf", *(o if o else ["unreached"]), g.leaf(n).hex())
        for i in (1, 8, 64, 83, 511) if o else ():
            v = [o[0] + i % 8, o[1] + i // 8 % 8, o[2] + i // 64]
            print("voxel", *v, g.get(v).hex())
    for level, o, t in tiles:
        print("tile", level, *o, g.tile(t).hex())


if __name__ == "__main__":
    args = sys.argv[1:]
    if args[:1] == ["--patch"]:
        base = open(args[1], "rb").read()
        for line in sys.stdin:
            offset, value = line.split()
            at, word = int(offset), int(value, 16)
            b = base[:at] + struct.pack("<I", word) + base[at + 4 :]
            read(f"{args[1]}@{offset}={value}", True, b)
    else:
        brief = "--brief" in args
        for path in (a for a in args if a != "--brief"):
            read(path, brief)
