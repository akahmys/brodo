#!/bin/sh
# Reproduces spikes/vdbvec/REPORT.md from the repository's root: builds the oracle against
# OpenVDB 12.0.0 and 13.0.0, as spikes/vdbread/REPORT.md builds them under
# out/spike/vdbread/inst/, and 14.0.0 as Homebrew installs it; writes every file, dumps it,
# decodes it, and reads each version's files with each version's oracle.
#
#   spikes/vdbvec/run.sh [OUT]          OUT defaults to out/spike/vdbvec
set -eu
O=${1:-out/spike/vdbvec}
I=out/spike/vdbread/inst
B=$(brew --prefix)
T=$(brew --prefix tbb)
mkdir -p "$O"

build() { # $1 the oracle's suffix, then the include and library directories
    out=$1; inc=$2; lib=$3; shift 3
    clang++ -std=c++17 -O2 -Wno-missing-template-arg-list-after-template-kw -I "$inc" -I"$B/include" \
        -I"$T/include" spikes/vdbvec/oracle/oracle.cpp -L "$lib" -L"$B/lib" -L"$T/lib" "$@" \
        -lopenvdb -ltbb -o "$O/oracle-$out"
}
build 12 "$I/openvdb-v12.0.0/include" "$I/openvdb-v12.0.0/lib" "-Wl,-rpath,$PWD/$I/openvdb-v12.0.0/lib"
build 13 "$I/openvdb-v13.0.0/include" "$I/openvdb-v13.0.0/lib" "-Wl,-rpath,$PWD/$I/openvdb-v13.0.0/lib"
build 14 "$B/include" "$B/lib"

for v in 12 13 14; do
    "$O/oracle-$v" info
    "$O/oracle-$v" types
done

# A half grid is decoded with its background, tiles and inactive values in full and its
# empty buffers unsized (REPORT.md, Findings 3); any other with neither.
modes="default zip none mask half half-zip half-none half-mask"
for v in 12 13 14; do
    for m in $modes; do
        "$O/oracle-$v" write "$O/w$v-$m.vdb" "$m"
        "$O/oracle-$v" dump "$O/w$v-$m.vdb" > "$O/d$v-$m.tsv"
        case $m in
            half*) python3 -I spikes/vdbvec/decode.py --empty --full f f "$O/w$v-$m.vdb" > "$O/x$v-$m.tsv" ;;
            *) python3 -I spikes/vdbvec/decode.py "$O/w$v-$m.vdb" > "$O/x$v-$m.tsv" ;;
        esac
        printf '%s %s: ' "$v" "$m"
        python3 -I spikes/vdbvec/decode.py --check "$O/x$v-$m.tsv" "$O/d$v-$m.tsv" | head -1
    done
done

# Each version's reading of each version's files, against the writer's own dump but for
# the file version's line; OpenVDB 12's warning on file version 225 goes to stderr.
for r in 12 13 14; do
    for w in 12 13 14; do
        for m in $modes; do
            "$O/oracle-$r" dump "$O/w$w-$m.vdb" 2>/dev/null | tail -n +2 > "$O/r.tsv"
            tail -n +2 "$O/d$w-$m.tsv" | cmp -s - "$O/r.tsv" || echo "read by $r, written by $w, $m: differs"
        done
    done
done
echo "cross-version reading checked"
