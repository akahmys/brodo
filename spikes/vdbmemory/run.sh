#!/bin/bash
# Writes each case with gen.py and prints its length and the reader's peak resident memory.
# Run from the repository's root: spikes/vdbmemory/run.sh <out dir>
set -euo pipefail
out=$1
mkdir -p "$out"
cargo build --release -q --manifest-path spikes/vdbmemory/Cargo.toml
bin=spikes/vdbmemory/target/release/vdbmemory-spike
printf 'case\tn\tbytes\tmax_rss\n'
for c in "tiles 0" "tiles 1" "tiles 4" "tiles 16" "tiles 64" "dtiles 16" "dtiles 64" \
         "leaves 4096" "leaves 32768" "leaves 262144" "fleaves 32768" "fleaves 262144" \
         "lowers 4096" "lowers 32768" "uppers 4096" "uppers 32768"; do
  read -r kind n <<< "$c"
  f=$out/${kind}_$n.vdb
  python3 -I spikes/vdbmemory/gen.py "$kind" "$n" "$f"
  case $kind in *leaves) l=$n t=0 ;; lowers|uppers) l=0 t=$n ;; *) l=0 t=$((32768 * n)) ;; esac
  rss=$( { /usr/bin/time -l "$bin" "$f" "$l" "$t"; } 2>&1 | awk '/maximum resident/{print $1}')
  printf '%s\t%s\t%s\t%s\n' "$kind" "$n" "$(stat -f %z "$f")" "$rss"
done
