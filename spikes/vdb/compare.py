"""Compares brodo's listing (OUT.tsv) with the oracle's dump of OUT, value by value.

    python3 -I spikes/vdb/compare.py LISTING DUMP

Exits 1 on the first difference in the background, the transform's matrix, or the set of
active boxes and their values, each value compared as a double, exactly.
"""
import sys


def boxes(lines):
    out = {}
    for line in lines:
        cols = line.split("\t")
        if len(cols) == 7 and all(c.lstrip("-").isdigit() for c in cols[:6]):
            out[tuple(map(int, cols[:6]))] = float(cols[6])
    return out


def field(lines, key):
    return next((l.split("\t")[1:] for l in lines if l.startswith(key + "\t")), None)


mine = open(sys.argv[1]).read().splitlines()
theirs = open(sys.argv[2]).read().splitlines()
bad = 0
if float(field(mine, "background")[0]) != float(field(theirs, "background")[0]):
    print("background differs"); bad += 1
a, b = boxes(mine), boxes(theirs)
missing = set(a) - set(b)
extra = set(b) - set(a)
wrong = [k for k in set(a) & set(b) if a[k] != b[k]]
print(f"brodo {len(a)} boxes, OpenVDB {len(b)}; missing {len(missing)}, extra {len(extra)}, values differing {len(wrong)}")
for k in list(missing)[:3] + list(extra)[:3] + wrong[:3]:
    print("  e.g.", k, a.get(k), b.get(k))
print("grid", field(theirs, "grid"))
print("map", field(theirs, "map"))
sys.exit(1 if bad or missing or extra or wrong else 0)
