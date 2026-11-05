#!/bin/bash
# Builds the writer against Homebrew's OpenVDB 14.0.0 and the checker against brodo, writes
# the level sets and the rotated ones, OpenVDB's and brodo's, and lists each grid as both read it.
# Run from the repository's root: spikes/vdbclass/run.sh <out dir>
set -euo pipefail
out=$1
mkdir -p "$out"
b=$(brew --prefix)
t=$(brew --prefix tbb)
clang++ -std=c++17 -O2 -I"$b/include" -I"$t/include" spikes/vdbclass/levelsets.cpp \
  -L"$b/lib" -L"$t/lib" -lopenvdb -ltbb -o "$out/levelsets"
cargo build --release -q --manifest-path spikes/vdbclass/Cargo.toml
check=spikes/vdbclass/target/release/vdbclass-spike
"$out/levelsets" "$out/levelsets.vdb" > "$out/by14.txt"
"$out/levelsets" rotations "$out/rotations.vdb" 2000
"$check" "$out/levelsets.vdb" > "$out/brodo.txt"
"$check" "$out/rotations.vdb" > "$out/rotations.txt"
"$check" brodo 100000 > "$out/brodo_rotations.txt"
cat "$out/by14.txt" "$out/brodo.txt"
python3 -I spikes/vdbclass/spread.py "$out/rotations.txt"
python3 -I spikes/vdbclass/spread.py "$out/brodo_rotations.txt"
