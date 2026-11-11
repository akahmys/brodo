"""Reads a `.vdb` file of float, double and vector grids whole, as a reader in brodo
would, and prints it as `spikes/vdbvec/oracle`'s `dump` does, so that the two listings can
be compared line for line. Copied from `spikes/vdbread/decode.py` and taken to values of
three components, each a tuple, negated component by component; a grid saved as half
floats read too, to learn its layout.

    python3 -I spikes/vdbvec/decode.py FILE > FILE.decoded
    python3 -I spikes/vdbvec/decode.py --empty --full f f HALF.vdb > HALF.decoded
    python3 -I spikes/vdbvec/decode.py --check FILE.decoded FILE.dump

Drawn on: docs/formats.md §9 for the header and the node layout; OpenVDB `v14.0.0`'s
`io/Compression.h` for the per-node indicator and a compressed buffer's signed byte count
(PROVENANCE.md); the LZ4 project's `lz4_Block_format.md` for LZ4's block; c-blosc's public
`README_CHUNK_FORMAT.rst` for a Blosc 1 chunk's header, block starts and splits. Which
inactive value each indicator gives, the maps' fields, and Blosc's split and shuffle rules
were taken from OpenVDB's output and held to its dump, not read.
"""
import struct
import sys
import zlib

# A tree type's component format and the components of a value.
TYPES = {
    "Tree_float_5_4_3": ("f", 1),
    "Tree_double_5_4_3": ("d", 1),
    "Tree_vec3s_5_4_3": ("f", 3),
    "Tree_vec3d_5_4_3": ("d", 3),
    "Tree_vec3s_5_4_3_HalfFloat": ("e", 3),
    "Tree_float_5_4_3_HalfFloat": ("e", 1),
}


def value_reader(kind, stored):
    """The format and byte size of one value of `kind`, its components stored as `stored`
    where a half grid's background and tiles may be stored in full."""
    fmt, n = TYPES[kind]
    if kind.endswith("_HalfFloat"):
        fmt = stored or fmt
    return fmt * n, struct.calcsize("<" + fmt * n)


def one(c, kind, stored=None):
    fmt, size = value_reader(kind, stored)
    return struct.unpack("<" + fmt, c.take(size))


def neg(v):
    return tuple(-x for x in v)
ZIP, MASK, BLOSC = 1, 2, 4


class Cursor:
    def __init__(self, data, at=0):
        self.data, self.at = data, at

    def take(self, n):
        out = self.data[self.at:self.at + n]
        if len(out) != n:
            raise ValueError(f"short read at {self.at}")
        self.at += n
        return out

    def u(self, fmt):
        return struct.unpack("<" + fmt, self.take(struct.calcsize(fmt)))[0]

    def string(self):
        return self.take(self.u("I")).decode()


def lz4_block(src, size):
    out = bytearray()
    i = 0
    while i < len(src):
        token = src[i]; i += 1
        n = token >> 4
        if n == 15:
            while True:
                b = src[i]; i += 1; n += b
                if b != 255:
                    break
        out += src[i:i + n]; i += n
        if i >= len(src):
            break
        offset = src[i] | src[i + 1] << 8; i += 2
        if offset == 0 or offset > len(out):
            raise ValueError("bad LZ4 offset")
        m = token & 15
        if m == 15:
            while True:
                b = src[i]; i += 1; m += b
                if b != 255:
                    break
        m += 4
        for _ in range(m):
            out.append(out[-offset])
    if len(out) != size:
        raise ValueError("an LZ4 block's size disagrees")
    return bytes(out)


def unshuffle(block, typesize):
    n = len(block) // typesize
    out = bytearray(len(block))
    for i in range(typesize):
        out[i:n * typesize:typesize] = block[i * n:(i + 1) * n]
    out[n * typesize:] = block[n * typesize:]
    return bytes(out)


def blosc(chunk):
    version, _, flags, typesize, nbytes, blocksize, cbytes = struct.unpack("<BBBBIII", chunk[:16])
    if cbytes != len(chunk) or version != 2:
        raise ValueError("a Blosc chunk's header disagrees")
    if flags & 2:
        return chunk[16:16 + nbytes]
    if flags >> 5 != 1 or flags & 4:
        raise ValueError(f"Blosc flags {flags:#x}: only LZ4 with byte shuffle is read")
    blocks = -(-nbytes // blocksize)
    starts = struct.unpack(f"<{blocks}i", chunk[16:16 + 4 * blocks])
    out = bytearray()
    for b, start in enumerate(starts):
        size = min(blocksize, nbytes - b * blocksize)
        splits = typesize if not flags & 0x10 and size == blocksize else 1
        at, block = start, bytearray()
        for _ in range(splits):
            part = size // splits
            n = struct.unpack("<i", chunk[at:at + 4])[0]; at += 4
            block += chunk[at:at + n] if n == part else lz4_block(chunk[at:at + n], part)
            at += n
        out += unshuffle(bytes(block), typesize) if flags & 1 else block
    return bytes(out)


def values(c, n, mask, flags, kind, background):
    """`n` values of a node, `mask` its active places, as `writeCompressedValues` puts them."""
    fmt, size = value_reader(kind, None)
    indicator = c.u("B")
    # An inactive value is stored as the tree's values are, half where they are, this to learn.
    inactive = [one(c, kind, INACTIVE) for _ in range({2: 1, 4: 1, 5: 2}.get(indicator, 0))]
    select = int.from_bytes(c.take(n // 8), "little") if indicator in (3, 4, 5) else 0
    count = n if indicator == 6 or not flags & MASK else bin(mask).count("1")
    raw = count * size
    if HALF_EMPTY_UNSIZED and kind.endswith("_HalfFloat") and count == 0:
        data = b""
    elif flags & (ZIP | BLOSC):
        stated = c.u("q")
        if stated <= 0:
            data = c.take(-stated if stated < 0 else raw)
        else:
            chunk = c.take(stated)
            data = blosc(chunk) if flags & BLOSC else zlib.decompress(chunk)
    else:
        data = c.take(raw)
    if len(data) != raw:
        raise ValueError("a buffer's value count disagrees")
    flat = struct.unpack("<" + fmt * count, data)
    k3 = len(fmt)
    stored = [tuple(flat[k3 * i:k3 * i + k3]) for i in range(count)]
    if count == n:
        return stored
    out, k = [], 0
    for p in range(n):
        if mask >> p & 1:
            out.append(stored[k]); k += 1
            continue
        s = select >> p & 1
        if indicator == 0:
            out.append(background)
        elif indicator == 1:
            out.append(neg(background))
        elif indicator == 2:
            out.append(inactive[0])
        elif indicator == 3:
            out.append(background if s else neg(background))
        elif indicator == 4:
            out.append(background if s else inactive[0])
        else:
            out.append(inactive[1] if s else inactive[0])
    return out


def place(p, n):
    return (p >> 2 * n, p >> n & (1 << n) - 1, p & (1 << n) - 1)


def transform(c):
    kind = c.string()
    m = [[1.0 if r == k else 0.0 for k in range(4)] for r in range(4)]
    if kind == "AffineMap":
        f = struct.unpack("<16d", c.take(128))
        m = [list(f[4 * r:4 * r + 4]) for r in range(4)]
    elif kind in ("ScaleMap", "UniformScaleMap"):
        f = struct.unpack("<15d", c.take(120))
        for k in range(3):
            m[k][k] = f[k]
    elif kind in ("ScaleTranslateMap", "UniformScaleTranslateMap"):
        f = struct.unpack("<18d", c.take(144))
        for k in range(3):
            m[3][k], m[k][k] = f[k], f[3 + k]
    elif kind == "TranslationMap":
        f = struct.unpack("<3d", c.take(24))
        for k in range(3):
            m[3][k] = f[k]
    else:
        raise ValueError(f"map {kind} not read")
    return kind, m


def g(v):
    return ",".join("%.17g" % x for x in v)


def grid(data, start, buffers, end, kind):
    c = Cursor(data, start)
    flags = c.u("I")
    meta = {}
    for _ in range(c.u("I")):
        key, typ = c.string(), c.string()
        meta[key] = (typ, c.take(c.u("I")))
    mapkind, m = transform(c)
    if c.u("i") != 1:
        raise ValueError("buffer count")
    bg = one(c, kind, BACKGROUND)
    rows, leaves = [], []
    tiles, children = c.u("I"), c.u("I")
    for _ in range(tiles):
        o = struct.unpack("<3i", c.take(12))
        v = one(c, kind, BACKGROUND)
        rows.append(f"tile\t{o[0]}\t{o[1]}\t{o[2]}\t4096\t{g(v)}:{c.u('B')}\n")

    def internal(origin, n, c_log, child):
        cmask = int.from_bytes(c.take((1 << 3 * n) // 8), "little")
        vmask = int.from_bytes(c.take((1 << 3 * n) // 8), "little")
        vals = values(c, 1 << 3 * n, vmask, flags, kind, bg)
        for p in range(1 << 3 * n):
            o = [origin[k] + (place(p, n)[k] << c_log) for k in range(3)]
            if cmask >> p & 1:
                child(o)
            else:
                rows.append(f"tile\t{o[0]}\t{o[1]}\t{o[2]}\t{1 << c_log}\t{g(vals[p])}:{vmask >> p & 1}\n")

    def leaf(o):
        leaves.append((o, int.from_bytes(c.take(64), "little")))

    for _ in range(children):
        o = struct.unpack("<3i", c.take(12))
        internal(o, 5, 7, lambda lo: internal(lo, 4, 3, leaf))
    if c.at != buffers:
        raise ValueError(f"the topology ends at {c.at}, not {buffers}")
    for o, mask in leaves:
        if int.from_bytes(c.take(64), "little") != mask:
            raise ValueError("a leaf's mask differs from its topology's")
        vals = values(c, 512, mask, flags, kind, bg)
        cells = "".join(f"\t{g(vals[p])}:{mask >> p & 1}" for p in range(512))
        rows.append(f"leaf\t{o[0]}\t{o[1]}\t{o[2]}{cells}\n")
    if c.at != end:
        raise ValueError(f"the leaf buffers end at {c.at}, not {end}")
    voxels = sum(bin(mask).count("1") for _, mask in leaves)
    voxels += sum(int(r.split("\t")[4]) ** 3 for r in rows if r.startswith("tile") and r.rstrip().endswith(":1"))
    return meta, mapkind, m, bg, voxels, sorted(rows)


# How a half grid's background and root tiles, and its inactive values, are stored: `None`
# as its values are, else a component format; set by `--full` to try the other.
BACKGROUND = None
INACTIVE = None
# Whether a half grid's buffer of no values is written with no size; set by `--empty`.
HALF_EMPTY_UNSIZED = False


def shown_meta(typ, v):
    """A metadata value as OpenVDB's `Metadata::str` shows it, for the types met here."""
    if typ == "string":
        return v.decode()
    if typ == "bool":
        return "true" if v[0] else "false"
    if typ == "int64":
        return str(struct.unpack("<q", v)[0])
    if typ == "vec3i":
        return "[" + ", ".join(str(x) for x in struct.unpack("<3i", v)) + "]"
    return v.hex()


def main(path):
    data = open(path, "rb").read()
    c = Cursor(data)
    if c.u("q") != 0x56444220:
        raise ValueError("not a .vdb file")
    version = c.u("I")
    c.take(8)
    if c.u("B") != 1:
        raise ValueError("no grid offsets")
    c.take(36)
    for _ in range(c.u("I")):
        c.string(); c.string(); c.take(c.u("I"))
    print(f"file\tversion\t{version}")
    out = {}
    for _ in range(c.u("i")):
        name, kind, instance = c.string(), c.string(), c.string()
        start, buffers, end = c.u("q"), c.u("q"), c.u("q")
        if instance:
            raise ValueError("an instanced grid")
        meta, mapkind, m, bg, voxels, rows = grid(data, start, buffers, end, kind)
        cls = meta.get("class", ("string", b"unknown"))[1].decode()
        vec = meta.get("vector_type", ("string", b"invariant"))[1].decode()
        half = int(kind.endswith("_HalfFloat"))
        shown = kind.removesuffix("_HalfFloat")
        listed = "".join(
            f"meta\t{k}\t{t}\t{shown_meta(t, v)}\n" for k, (t, v) in sorted(meta.items())
            # OpenVDB 12 and 13 write a `__delayedload` entry, which their reader keeps to itself.
            if t != "__delayedload"
        )
        out[name] = (f"grid\t{name}\ttype\t{shown}\tclass\t{cls}\tvector_type\t{vec}\thalf\t{half}\tvoxels\t{voxels}\n"
                     + listed
                     + "map\t" + mapkind + "".join("\t%.17g" % m[r][k] for r in range(4) for k in range(4))
                     + f"\nbackground\t{g(bg)}\n" + "".join(rows))
        c.at = end
    # OpenVDB's dump lists grids by name.
    sys.stdout.write("".join(out[k] for k in sorted(out)))


def check(mine, theirs):
    a, b = open(mine).read().splitlines(), open(theirs).read().splitlines()
    if a == b:
        print(f"same: {len(a)} lines")
        return 0
    diff = [k for k in range(min(len(a), len(b))) if a[k] != b[k]]
    print(f"differ: {len(a)} lines against {len(b)}, {len(diff)} differing; first:")
    for k in diff[:3]:
        print("  mine  ", a[k][:160]); print("  theirs", b[k][:160])
    return 1


if sys.argv[1] == "--check":
    sys.exit(check(sys.argv[2], sys.argv[3]))
if sys.argv[1] == "--empty":
    HALF_EMPTY_UNSIZED = True
    del sys.argv[1]
if sys.argv[1] == "--full":
    BACKGROUND, INACTIVE = sys.argv[2], sys.argv[3]
    sys.argv[1:4] = sys.argv[4:5]
main(sys.argv[1])
