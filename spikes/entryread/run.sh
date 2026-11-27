#!/bin/bash
set -euo pipefail
# Writes the cases, times them in two processes, and gives each case's and variant's peak
# resident memory. Run from the repository's root: spikes/entryread/run.sh ROUNDS
S=spikes/entryread; O=out/spike/entryread; B=$S/target/release/entryread-spike
mkdir -p "$O"
cargo build --release -q --manifest-path $S/Cargo.toml
echo "load $(sysctl -n vm.loadavg)"
"$B" write > "$O/cases.tsv"
"$B" time "$1" > "$O/run-a.tsv"
echo "load $(sysctl -n vm.loadavg)"
"$B" time "$1" > "$O/run-b.tsv"
echo "load $(sysctl -n vm.loadavg)"
printf 'case\tvariant\tmax_rss\n' > "$O/rss.tsv"
for c in one_large four_large one_medium many_medium many_small; do
  for v in none brodo whole entry; do
    rss=$( { /usr/bin/time -l "$B" read "$c" "$v"; } 2>&1 | awk '/maximum resident/{print $1}')
    printf '%s\t%s\t%s\n' "$c" "$v" "$rss" >> "$O/rss.tsv"
  done
done
