#!/usr/bin/env bash
# Release-mode AppendEntries benchmark: tonic vs QuicCrunch.
#
# Three repeats with alternating transport order. The benchmark host is
# shared and may be busy: this script does NOT wait for a quiet window. Every
# cell records the 1-minute load average at start and end, and the docs report
# medians together with the measured load range. Numbers from a loaded host
# are labelled provisional.
#
# Run inside the dev shell:
#   nix develop -c ./scripts/bench_raft_vs_grpc.sh
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${OUT:-benchmarks/raft_append_vs_grpc.csv}
DURATION_MS=${BENCH_DURATION_MS:-3000}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-qc-raft}
export TMPDIR=${TMPDIR:-/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.tmp-qc-raft}
mkdir -p "$TMPDIR" "$(dirname "$OUT")"

echo "repeat,transport,payload_bytes,depth,ops,rate_per_s,p50_us,p95_us,p99_us,e2e_p99_us,enqueue_p99_us,server_p99_us,sched_late_p99_us,load_start,load_end,errors" > "$OUT"

# Build first. No measurement happens until the binary exists.
cargo build --release -q -p raft-compare
BIN="$CARGO_TARGET_DIR/release/raft-compare"

for repeat in 1 2 3; do
  # Alternate the order so a thermal or load drift does not favour one
  # transport in every repeat.
  if (( repeat % 2 == 1 )); then
    order=(tonic quic)
  else
    order=(quic tonic)
  fi
  for transport in "${order[@]}"; do
    echo "# repeat=$repeat transport=$transport load=$(cut -d' ' -f1 /proc/loadavg)" >&2
    BENCH_REPEAT="$repeat" "$BIN" "$transport" "$DURATION_MS" | grep -v '^repeat,' | tee -a "$OUT"
  done
done

echo "wrote $OUT" >&2