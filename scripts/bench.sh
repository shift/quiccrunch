#!/usr/bin/env bash
# Reproducible QuicCrunch benchmark.
#
# Every number in docs/PERFORMANCE_RELEASE.md comes from this script.
#
# Usage:
#   scripts/bench.sh                 # default matrix, spread distribution
#   MODE=reuseport scripts/bench.sh  # kernel SO_REUSEPORT hash instead
#   WORKERS="1 2 4 6" CONNS="8 16 32 64" DEPTHS="1 64 1800" \
#     DURATION=10 WARMUP=3 RUNS=1 scripts/bench.sh
#
# Environment:
#   MODE=spread|reuseport   (default spread: worker w listens on port+w)
#   WORKERS  space separated worker counts        (default "4")
#   CONNS    space separated connection counts    (default "8 16 32 64")
#   DEPTHS   space separated pipeline depths      (default "1 64 1800")
#   STREAMS  streams per connection               (default 4)
#   SIZE     message payload bytes                (default 1024)
#   DURATION measurement seconds                  (default 10)
#   WARMUP   warm-up seconds                      (default 3)
#   RUNS     repetitions per cell                 (default 1)
#   BASE_PORT first listen port                   (default 4600)
#   RUST_LOG log level for both processes         (default warn)
#   CARGO_TARGET_DIR target directory
set -euo pipefail

cd "$(dirname "$0")/.."

MODE="${MODE:-spread}"
WORKERS="${WORKERS:-4}"
CONNS="${CONNS:-8 16 32 64}"
DEPTHS="${DEPTHS:-1 64 1800}"
STREAMS="${STREAMS:-4}"
# Nix devshells export SIZE=size, which is not a byte count.
SIZE="${MSG_SIZE:-${SIZE:-1024}}"
if ! [[ "$SIZE" =~ ^[0-9]+$ ]]; then SIZE=1024; fi
DURATION="${DURATION:-10}"
WARMUP="${WARMUP:-3}"
RUNS="${RUNS:-1}"
BASE_PORT="${BASE_PORT:-4600}"
export RUST_LOG="${RUST_LOG:-warn}"
TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
BIN="$TARGET_DIR/release"
CERT="$PWD/quiccrunch-core/tests/certs/cert.pem"
KEY="$PWD/quiccrunch-core/tests/certs/key.pem"

if [[ ! -f "$CERT" || ! -f "$KEY" ]]; then
  mkdir -p "$(dirname "$CERT")"
  openssl req -x509 -newkey rsa:2048 -keyout "$KEY" -out "$CERT" \
    -days 365 -nodes -subj '/CN=localhost' >/dev/null 2>&1
fi

echo "=== QuicCrunch benchmark ==="
echo "host: $(awk -F: '/model name/{gsub(/^ +/,"",$2); print $2; exit}' /proc/cpuinfo)"
echo "cores: $(nproc)  kernel: $(uname -r)  governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
echo "build: release  mode: $MODE  RUST_LOG=$RUST_LOG"
echo "cell: streams/conn=$STREAMS msg_size=$SIZE warmup=${WARMUP}s duration=${DURATION}s runs=$RUNS"
echo

if [[ ! -x "$BIN/multi_worker_server" || ! -x "$BIN/pipelined_benchmark" ]]; then
  echo "building release binaries..."
  cargo build --release --bin multi_worker_server --bin pipelined_benchmark
fi

printf '%-3s %-5s %-9s %-10s %-8s %-8s %-8s %-8s %-6s %-6s %s\n' \
  "w" "conns" "depth" "msg/s" "p50us" "p95us" "p99us" "maxus" "err" "timeout" "distribution"

run_cell() {
  local workers="$1" conns="$2" depth="$3" port="$4"
  local server_args=( "$BIN/multi_worker_server" "127.0.0.1:$port" "$CERT" "$KEY" "$workers" --bench )
  local client_args=( "$BIN/pipelined_benchmark" "127.0.0.1:$port" "$DURATION" "$conns" "$STREAMS" "$depth" "$SIZE"
                      --warmup "$WARMUP" --workers "$workers" --json )
  if [[ "$MODE" == "spread" ]]; then
    server_args+=( --spread )
    client_args+=( --spread "$workers" )
  fi

  RUST_LOG="$RUST_LOG" "${server_args[@]}" >"$TARGET_DIR/server-bench.log" 2>&1 &
  local spid=$!
  # Wait for all worker sockets to be listening.
  for _ in $(seq 1 100); do
    local listening
    listening=$(ss -lun 2>/dev/null | grep -c ":$port " || true)
    [[ "$listening" -ge 1 ]] && break
    sleep 0.1
  done
  sleep 0.3

  local out rc
  set +e
  out=$(RUST_LOG="$RUST_LOG" timeout $((WARMUP + DURATION + 90)) "${client_args[@]}" 2>&1)
  rc=$?
  set -e

  kill "$spid" 2>/dev/null || true
  wait "$spid" 2>/dev/null || true

  if [[ $rc -ne 0 ]]; then
    echo "benchmark failed (rc=$rc): $out" >&2
    return 1
  fi

  local json
  json=$(printf '%s\n' "$out" | grep '^{' | tail -1)
  # Fall back to grep if jq is unavailable.
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "$json" | jq -r --arg w "$workers" --arg c "$conns" --arg d "$depth" --arg mode "$MODE" \
      '[$w,$c,$d,(.throughput_msg_s|floor),.rtt_p50_us,.rtt_p95_us,.rtt_p99_us,((.rtt_p999_us*2)|floor/2),.errors,.timeouts,$mode] | @tsv' \
      | awk -F'\t' '{printf "%-3s %-5s %-9s %-10s %-8.0f %-8.0f %-8.0f %-8.0f %-6s %-6s %s\n",$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11}'
  else
    printf '%s\n' "$json"
  fi
}

port=$BASE_PORT
for workers in $WORKERS; do
  for conns in $CONNS; do
    for depth in $DEPTHS; do
      for ((r = 1; r <= RUNS; r++)); do
        [[ $RUNS -gt 1 ]] && echo "# run $r"
        run_cell "$workers" "$conns" "$depth" "$port" || true
        port=$((port + workers + 2))
      done
    done
  done
done

echo
echo "Note: MODE=spread binds worker w to port+w so connection i goes to worker i%N."
echo "MODE=reuseport uses the kernel 4-tuple hash; with 16 connections the hash"
echo "is uneven (measured 10/4/2/0 across four workers), so totals are noisy."