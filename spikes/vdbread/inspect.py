"""Lists what a `.vdb` file's bytes hold that a reader must decode: the file version, each
grid's compression flags and metadata, and over its leaf buffers each per-node indicator
and, for a compressed buffer, its codec.

    python3 -I spikes/vdbread/inspect.py FILE

The header and grid layout are docs/formats.md §9's; the leaf buffer's indicator, the
inactive values and selection mask before it, and a compressed buffer's signed `i64` byte
count, are as OpenVDB `v14.0.0`'s `io/Compression.h` reads them (PROVENANCE.md). A Blosc
buffer's header is c-blosc 1's, as its public `README_HEADER.rst` states. The leaf buffers
are walked to the body's end: a walk that does not end there was misread.
"""
import collections
import struct
import sys

SIZE = {"Tree_float_5_4_3": 4, "Tree_double_5_4_3": 8}
BLOSC_CODEC = {0: "blosclz", 1: "lz4", 2: "snappy", 3: "zlib", 4: "zstd"}
FLAGS = {1: "zip", 2: "active-mask", 4: "blosc"}


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


def codec(chunk, flags):
    """The codec of one compressed buffer, checked against its stated size."""
    if flags & 4:
        version, _, bflags, typesize, nbytes, _, cbytes = struct.unpack("<BBBBIII", chunk[:16])
        if cbytes != len(chunk):
            raise ValueError("a Blosc buffer's size disagrees")
        name = BLOSC_CODEC.get(bflags >> 5, f"code {bflags >> 5}")
        shuffle = "shuffle" if bflags & 1 else ("bitshuffle" if bflags & 4 else "noshuffle")
        memcpy = " memcpyed" if bflags & 2 else ""
        return f"blosc{version} {name} {shuffle}{memcpy} typesize {typesize}", nbytes
    if flags & 1:
        return f"zlib header {chunk[:2].hex()}", None
    return "none", None


def leaves(c, end, size, flags):
    counts = collections.Counter()
    while c.at < end:
        mask = int.from_bytes(c.take(64), "little")
        active = bin(mask).count("1")
        indicator = c.u("B")
        inactive = {2: 1, 4: 1, 5: 2}.get(indicator, 0)
        c.take(inactive * size)
        if indicator in (3, 4, 5):
            c.take(64)
        n = 512 if indicator == 6 or not flags & 2 else active
        if flags & 5:
            count = c.u("q")
            if count <= 0:
                c.take(-count if count < 0 else n * size)
                counts[(indicator, "stored raw")] += 1
                continue
            name, nbytes = codec(c.take(count), flags)
            if nbytes is not None and nbytes != n * size:
                raise ValueError("a compressed buffer's value count disagrees")
            counts[(indicator, name)] += 1
        else:
            c.take(n * size)
            counts[(indicator, "none")] += 1
    if c.at != end:
        raise ValueError(f"the leaf buffers end at {c.at}, not {end}")
    return counts


def main(path):
    data = open(path, "rb").read()
    c = Cursor(data)
    if c.u("q") != 0x56444220:
        raise ValueError("not a .vdb file")
    version = c.u("I")
    library = (c.u("I"), c.u("I"))
    print(f"file version {version} library {library[0]}.{library[1]}")
    if c.u("B") != 1:
        raise ValueError("no grid offsets")
    c.take(36)
    for _ in range(c.u("I")):
        print("file meta", c.string(), c.string(), c.take(c.u("I")).hex())
    for _ in range(c.u("i")):
        name, kind, instance = c.string(), c.string(), c.string()
        start, buffers, end = c.u("q"), c.u("q"), c.u("q")
        body = Cursor(data, start)
        flags = body.u("I")
        print(f"grid {name} {kind} instance {instance!r} flags {flags}",
              "+".join(v for k, v in FLAGS.items() if flags & k) or "none")
        for _ in range(body.u("I")):
            key, typ = body.string(), body.string()
            value = body.take(body.u("I"))
            shown = value.decode() if typ == "string" else value.hex()
            print(f"  meta {key} {typ} {shown}")
        for (indicator, name), n in sorted(leaves(Cursor(data, buffers), end, SIZE[kind], flags).items()):
            print(f"  leaves indicator {indicator} {name}: {n}")
        c.at = end


main(sys.argv[1])
