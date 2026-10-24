#!/usr/bin/env bash
# Renders the epicycles with manim and with fastanim from Python, and with fastanim from Rhai,
# for each number of vectors given (default 10 50 100 200), and prints the wall-clock seconds
# each took.
#
#   QUALITY=1080p60 DURATION=10 ./bench.sh 10 50 100 200
#
# Needs `manim` and the `fastanim` package with its preview extra on PYTHON (default python3),
# the `fastanim` CLI as FASTANIM (default: fastanim on PATH), and the Rust example as RUST
# (`cargo build --release -p fastanim-bevy --example epicycles`).
set -euo pipefail
cd "$(dirname "$0")"

PYTHON=${PYTHON:-python3}
FASTANIM=${FASTANIM:-fastanim}
RUST=${RUST:-../../target/release/examples/epicycles}
QUALITY=${QUALITY:-1080p60}
export DURATION=${DURATION:-10}
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

case $QUALITY in
480p15) mflag=-ql ;;
720p30) mflag=-qm ;;
1080p60) mflag=-qh ;;
4k60) mflag=-qk ;;
*) echo "unknown QUALITY $QUALITY" >&2; exit 1 ;;
esac

seconds() {
    local start=$(date +%s.%N)
    "$@" >"$out/log" 2>&1 || { cat "$out/log" >&2; return 1; }
    echo "$(date +%s.%N) - $start" | bc
}

printf '%6s %10s %10s %10s %10s %8s\n' N manim fastanim rhai rust speedup
[ $# -gt 0 ] || set -- 10 50 100 200
for N in "$@"; do
    export N
    m=$(seconds manim "$mflag" --disable_caching --media_dir "$out/manim" manim_epicycles.py Epicycles)
    f=$(seconds "$PYTHON" -m fastanim render fastanim_epicycles.py -q "$QUALITY" -o "$out/fastanim.mp4")
    # Rhai can't read the environment, so N and DURATION are written into a copy.
    [[ $DURATION == *.* ]] && d=$DURATION || d=$DURATION.0
    sed "s/^let n = .*;/let n = $N;/; s/^let duration = .*;/let duration = $d;/" epicycles.rhai >"$out/epicycles.rhai"
    r=$(seconds "$FASTANIM" render "$out/epicycles.rhai" -q "$QUALITY" -o "$out/rhai.mp4")
    x=$(seconds "$RUST" render -q "$QUALITY" -o "$out/rust.mp4")
    printf '%6d %9.1fs %9.1fs %9.1fs %9.1fs %7.1fx\n' "$N" "$m" "$f" "$r" "$x" "$(echo "$m / $f" | bc -l)"
done
