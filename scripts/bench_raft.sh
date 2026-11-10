#!/usr/bin/env bash
# Three-run AppendEntries benchmark, medians reported by the caller.
# The host is shared; every row records the 1-minute load average.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${OUT:-benchmarks/raft_append_quic.csv}
DURATION_MS=${BENCH_DURATION_MS:-5000}
PAYLOAD=${BENCH_PAYLOAD:-512}
export BENCH_DURATION_MS="$DURATION_MS"
export BENCH_PAYLOAD="$PAYLOAD"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-qc-raft}

mkdir -p "$(dirname "$OUT")"
echo "transport,load1,ops,rate_per_s,p50_us,p95_us,p99_us,errors" > "$OUT"

nix develop -c cargo build -q -p quiccrunch-core --example raft_append_bench
BIN="$CARGO_TARGET_DIR/debug/examples/raft_append_bench"

for run in 1 2 3; do
  echo "# run $run load=$(cut -d' ' -f1 /proc/loadavg)" >&2
  "$BIN" | grep '^quic,' | tee -a "$OUT"
done

echo "wrote $OUT" >&2