# Writes .vdb files, as docs/formats.md §9 states, that make the most memory per byte
# read: tiles of one upper node after another, or leaves holding the background; and nodes
# each holding one tile, to measure what a node makes.
# Usage: gen.py tiles|dtiles|leaves|fleaves|lowers|uppers <n> <out>
import struct, sys
def s(b): return struct.pack('<I', len(b)) + b
def file(kind, typ, body_fn):
    hdr = struct.pack('<qI', 0x56444220, 225) + struct.pack('<II',14,0) + b'\x01' + b'0'*36 + struct.pack('<Ii',0,1)
    desc_pre = s(b'g') + s(typ) + s(b'')
    start = len(hdr) + len(desc_pre) + 24
    topo, bufs = body_fn()
    body_head = struct.pack('<I', 2) + struct.pack('<I', 0) + s(b'AffineMap') + struct.pack('<16d', 1,0,0,0, 0,1,0,0, 0,0,1,0, .5,.5,.5,1)
    buffers = start + len(body_head) + len(topo)
    end = buffers + len(bufs)
    return hdr + desc_pre + struct.pack('<qqq', start, buffers, end) + body_head + topo + bufs
def tiles(k, d=False):  # k upper nodes, each 32768 inactive tiles of a non-background value
    def body():
        t = struct.pack('<i', 1) + struct.pack('<d' if d else '<f', 0.0) + struct.pack('<II', 0, k)
        for i in range(k):
            t += struct.pack('<3i', 4096*i, 0, 0) + bytes(4096) + bytes(4096) + b'\x02' + struct.pack('<d' if d else '<f', 1.0)
        return t, b''
    return file('tiles', b'Tree_double_5_4_3' if d else b'Tree_float_5_4_3', body)
def leaves(n, d=True):  # n leaves, every one inactive at the background, f64 or f32
    def body():
        t = struct.pack('<i', 1) + struct.pack('<d' if d else '<f', 0.0) + struct.pack('<II', 0, 1)
        lowers = (n + 4095) // 4096
        assert lowers <= 32768
        um = bytearray(4096)
        for p in range(lowers): um[p//8] |= 1 << (p%8)
        t += struct.pack('<3i', 0,0,0) + bytes(um) + bytes(4096) + b'\x00'
        left = n; b = b''
        for _ in range(lowers):
            m = min(left, 4096); left -= m
            lm = bytearray(512)
            for p in range(m): lm[p//8] |= 1 << (p%8)
            t += bytes(lm) + bytes(512) + b'\x00' + bytes(64)*m
            b += (bytes(64) + b'\x00') * m
        return t, b
    return file('leaves', b'Tree_double_5_4_3' if d else b'Tree_float_5_4_3', body)

def nodes(kind, n):  # lowers: one upper of n lowers; uppers: n uppers of one lower; each lower one active tile, f32
    def lower():
        vm = bytearray(512); vm[0] = 1
        return bytes(512) + bytes(vm) + b'\x00' + struct.pack('<f', 1.0)
    def upper(o, m):
        cm = bytearray(4096)
        for q in range(m): cm[q//8] |= 1 << (q%8)
        return struct.pack('<3i', *o) + bytes(cm) + bytes(4096) + b'\x00' + lower() * m
    def body():
        k = 1 if kind == 'lowers' else n
        t = struct.pack('<i', 1) + struct.pack('<f', 0.0) + struct.pack('<II', 0, k)
        for i in range(k):
            t += upper((4096*i, 0, 0), n if kind == 'lowers' else 1)
        return t, b''
    return file(kind, b'Tree_float_5_4_3', body)
kind, n, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
open(out, 'wb').write(nodes(kind, n) if kind in ('lowers', 'uppers') else tiles(n) if kind == 'tiles' else tiles(n, True) if kind == 'dtiles' else leaves(n, kind == 'leaves'))
