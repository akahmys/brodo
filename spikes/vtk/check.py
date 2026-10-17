"""Reads each candidate with VTK's Python package and compares cell values at the samples
the spike wrote (cell centres and expected values); times each read. Run with the venv's
interpreter: out/vtk-venv/bin/python -I spikes/vtk/check.py out/vtk."""

import glob
import math
import os
import struct
import sys
import time

import vtk


def f32(x):
    return struct.unpack("f", struct.pack("f", x))[0]


def same(got, want):
    """Exact at Float32 precision; a NaN matches a NaN."""
    if math.isnan(want):
        return math.isnan(got)
    return f32(got) == f32(want)


def samples(base):
    with open(os.path.join(base, "samples.txt")) as f:
        return [tuple(float(x) for x in line.split()) for line in f]


def check_vtu(base, pts):
    start = time.perf_counter()
    r = vtk.vtkXMLUnstructuredGridReader()
    r.SetFileName(os.path.join(base, "sphere.vtu"))
    r.Update()
    grid = r.GetOutput()
    took = time.perf_counter() - start
    values = grid.GetCellData().GetArray("value")
    loc = vtk.vtkCellLocator()
    loc.SetDataSet(grid)
    loc.BuildLocator()
    bad = 0
    for x, y, z, v in pts:
        cid = loc.FindCell((x, y, z))
        if cid < 0 or not same(values.GetValue(cid), v):
            bad += 1
    vols = [grid.GetCell(i).GetBounds() for i in range(0, grid.GetNumberOfCells(), 997)]
    flat = sum(1 for b in vols if b[1] <= b[0] or b[3] <= b[2] or b[5] <= b[4])
    return grid.GetNumberOfCells(), took, bad, flat


def check_vti(base, pts):
    start = time.perf_counter()
    images = []
    for name in sorted(glob.glob(os.path.join(base, "vti", "*.vti"))):
        r = vtk.vtkXMLImageDataReader()
        r.SetFileName(name)
        r.Update()
        images.append(r.GetOutput())
    took = time.perf_counter() - start
    cells = sum(im.GetNumberOfCells() for im in images)
    bad = 0
    for x, y, z, v in pts:
        found = None
        for im in images:
            b = im.GetBounds()
            if b[0] <= x <= b[1] and b[2] <= y <= b[3] and b[4] <= z <= b[5]:
                ijk, pc = [0, 0, 0], [0.0, 0.0, 0.0]
                cid = im.ComputeCellId(ijk) if im.ComputeStructuredCoordinates((x, y, z), ijk, pc) else -1
                if cid >= 0:
                    found = im.GetCellData().GetArray("value").GetValue(cid)
                    break
        if found is None or not same(found, v):
            bad += 1
    return cells, took, bad


for base in sorted(glob.glob(os.path.join(sys.argv[1], "r*"))):
    pts = samples(base)
    n, took, bad, flat = check_vtu(base, pts)
    print(f"{os.path.basename(base)} vtu: {n} cells, read {took:.2f} s, {bad} of {len(pts)} samples wrong, {flat} flat cells of those checked")
    n, took, bad = check_vti(base, pts)
    print(f"{os.path.basename(base)} vti: {n} cells, read {took:.2f} s, {bad} of {len(pts)} samples wrong")
