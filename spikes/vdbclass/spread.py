"""Summarises the checker's listing of the rotated level sets: how many grids meet option B
with the voxel sizes equal within 16 ε and exactly, and the spread's median, 99th centile
and greatest, in units of ε. Usage: python3 -I spread.py LISTING"""

import re
import sys

spreads, within, exact = [], 0, 0
for line in open(sys.argv[1], encoding="utf-8"):
    m = re.search(r"spread=([\d.]+)eps", line)
    if m:
        spreads.append(float(m.group(1)))
        within += "B16=true" in line
        exact += "Bexact=true" in line
spreads.sort()
n = len(spreads)
print(f"{n} grids; within 16 eps {within}, exact {exact}; spread median {spreads[n // 2]}, "
      f"p99 {spreads[int(0.99 * n)]}, greatest {spreads[-1]} eps")
