#!/bin/bash
# Prints, for each layout and radius, the counts, the seconds taken and the peak resident
# memory. Run from the repository's root, at rest: spikes/hermitesparse/run.sh <out dir>
set -euo pipefail
S=spikes/hermitesparse; O=$1; mkdir -p "$O"
cargo build --release -q --manifest-path $S/Cargo.toml
B=$S/target/release/hermitesparse-spike
printf 'layout\tradius\tleaves\ttiles\tcrossings\tbuilt_s\tfilled_s\twritten_s\tbytes\tmax_rss\n'
for c in "none 200" "dense 200" "sparse 200" "none 600" "dense 600" "sparse 600" \
         "none 1500" "sparse 1500"; do
  read -r l r <<< "$c"; f=$O/$l-$r.bin
  line=$( { /usr/bin/time -l "$B" "$l" "$r" "$f" > "$O/line"; } 2>&1 | awk '/maximum resident/{print $1}')
  size=$( [ -f "$f" ] && stat -f %z "$f" || echo 0 )
  printf '%s\t%s\t%s\t%s\t%s\n' "$l" "$r" "$(cat "$O/line")" "$size" "$line"
  rm -f "$f"
done
