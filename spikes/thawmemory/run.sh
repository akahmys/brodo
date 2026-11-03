#!/bin/bash
set -euo pipefail
# Writes each case and prints its length, its counts and mestolo info's peak resident memory.
# Run from the repository's root: spikes/thawmemory/run.sh <out dir>
S=spikes/thawmemory; O=$1; mkdir -p "$O"
cargo build --release -q --manifest-path $S/Cargo.toml
cargo build --release -q -p mestolo
M=target/release/mestolo
printf 'case\tn\tbytes\tmax_rss\tcounts\n'
for c in "tiles 1" "tiles 16" "nodes 4096" "nodes 32768" "leaves 32768" "leaves 262144"; do
  read -r k n <<< "$c"; f=$O/${k}_$n.cache
  counts=$("$S/target/release/thawmemory-spike" "$k" "$n" "$f")
  rss=$( { /usr/bin/time -l "$M" info "$f" --limit 100000000000 >/dev/null; } 2>&1 | awk '/maximum resident/{print $1}')
  printf '%s\t%s\t%s\t%s\t%s\n' "$k" "$n" "$(stat -f %z "$f")" "$rss" "$counts"
done
