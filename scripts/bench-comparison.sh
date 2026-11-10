#!/usr/bin/env bash
# Same-shape head-to-head: tonic (h2 over TCP+TLS) vs QuicCrunch (tQUIC/TLS).
#
# Run it from the repository root through the dev shell:
#   nix develop -c bash scripts/bench-comparison.sh
#
# Environment:
#   CONNS="1 8 16 64"      connection counts
#   DEPTHS="1 64 1800"     per-stream pipeline depths
#   DELAYS="0 1 10"        one-way delay in ms (0 = direct loopback)
#   WORKERS=4              server workers
#   DURATION=5 WARMUP=3    measurement / warm-up seconds
#   RUNS=3                 repetitions per cell (medians are reported)
#   QUIC_ONLY=0            skip tonic
#   WORKER_SCALING=1       add the 1-worker QuicCrunch scaling cells
#   CALIBRATE=1            run the delay-proxy overhead calibration
#   OUT=comparison/results/matrix.csv
#   RUST_LOG=warn          effective level for both sides (do not raise)
#
# CARGO_TARGET_DIR must point at the large volume; see comparison/README.md.
set -euo pipefail

cd "$(dirname "$0")/.."

BIN="${CARGO_TARGET_DIR:-$PWD/target}/release"
CERT_DIR="$PWD/comparison/certs"
CERT="$CERT_DIR/server.pem"
KEY="$CERT_DIR/server.key"
CA="$CERT_DIR/ca.pem"
CONNS="${CONNS:-1 8 16 64}"
DEPTHS="${DEPTHS:-1 64 1800}"
DELAYS="${DELAYS:-0 1 10}"
WORKERS="${WORKERS:-4}"
DURATION="${DURATION:-5}"
WARMUP="${WARMUP:-3}"
RUNS="${RUNS:-3}"
STREAMS="${STREAMS:-4}"
MSG_SIZE="${MSG_SIZE:-1024}"
BATCH="${BATCH:-64}"
BASE_PORT="${BASE_PORT:-4700}"
OUT="${OUT:-comparison/results/matrix.csv}"
CALIBRATE="${CALIBRATE:-1}"
WORKER_SCALING="${WORKER_SCALING:-1}"
QUIC_ONLY="${QUIC_ONLY:-0}"
export RUST_LOG="${RUST_LOG:-warn}"
GRPC_CLIENT_THREADS="${GRPC_CLIENT_THREADS:-10}"
QUIC_CLIENT_RUNTIMES="${QUIC_CLIENT_RUNTIMES:-8}"
QUIC_CLIENT_THREADS="${QUIC_CLIENT_THREADS:-2}"

mkdir -p "$(dirname "$OUT")"

# ---------------------------------------------------------------- certificates
if [[ ! -f "$CERT" || ! -f "$KEY" || ! -f "$CA" ]]; then
  echo "generating throwaway CA + localhost certificate in $CERT_DIR"
  mkdir -p "$CERT_DIR"
  (
    cd "$CERT_DIR"
    openssl genrsa -out ca.key 2048 >/dev/null 2>&1
    openssl req -x509 -new -key ca.key -sha256 -days 3650 \
      -subj '/CN=QuicCrunch Comparison CA' -out ca.pem >/dev/null 2>&1
    openssl genrsa -out server.key 2048 >/dev/null 2>&1
    openssl req -new -key server.key -subj '/CN=localhost' \
      -out server.csr >/dev/null 2>&1
    printf 'subjectAltName=DNS:localhost,DNS:127.0.0.1,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n' > ext.cnf
    openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
      -out server.pem -days 3650 -sha256 -extfile ext.cnf >/dev/null 2>&1
  )
fi

# ---------------------------------------------------------------------- build
if [[ ! -x "$BIN/compare-client" || ! -x "$BIN/grpc-server" || \
      ! -x "$BIN/delay-proxy" || ! -x "$BIN/multi_worker_server" ]]; then
  echo "building release binaries..."
  cargo build --release -p comparison -p quiccrunch-core \
    --bin compare-client --bin grpc-server --bin delay-proxy --bin multi_worker_server
fi

echo "=== tonic vs QuicCrunch comparison ==="
echo "host:  $(awk -F: '/model name/{gsub(/^ +/,"",$2); print $2; exit}' /proc/cpuinfo)"
echo "cores: $(nproc)  kernel: $(uname -r)  governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
echo "build: release  RUST_LOG=$RUST_LOG"
echo "cell:  streams/conn=$STREAMS msg=$MSG_SIZE warmup=${WARMUP}s duration=${DURATION}s runs=$RUNS"
echo "load:  $(cat /proc/loadavg)"
echo

printf '%s\n' \
  "transport,workers,connections,depth,delay_ms,msg_size,run,loadavg,log_level,throughput_msg_s,rtt_p50_us,rtt_p95_us,rtt_p99_us,rtt_p999_us,e2e_p50_us,e2e_p99_us,queue_p50_us,errors,timeouts,cutoff" > "$OUT"

PORT="$BASE_PORT"
RUN_ID=0

wait_tcp_listen() {
  local port="$1"
  for _ in $(seq 1 150); do
    if ss -ltn 2>/dev/null | grep -q ":$port "; then return 0; fi
    sleep 0.05
  done
  echo "timed out waiting for TCP $port" >&2
  return 1
}

wait_udp_listen() {
  local port="$1"
  for _ in $(seq 1 150); do
    if ss -lun 2>/dev/null | grep -q ":$port "; then return 0; fi
    sleep 0.05
  done
  echo "timed out waiting for UDP $port" >&2
  return 1
}

# run_cell TRANSPORT WORKERS CONNS DEPTH DELAY MSG_SIZE RUN
run_cell() {
  local transport="$1" workers="$2" conns="$3" depth="$4" delay="$5" size="$6" run="$7"
  local server_port=$((PORT + 2 * RUN_ID))
  local client_port="$server_port"
  local server_pid="" proxy_pid="" proxy_listen=""
  RUN_ID=$((RUN_ID + 1))

  if [[ "$transport" == "quic" ]]; then
    RUST_LOG="$RUST_LOG" "$BIN/multi_worker_server" "127.0.0.1:$server_port" \
      "$CERT" "$KEY" "$workers" --bench >"$OUT.server.log" 2>&1 &
    server_pid=$!
    wait_udp_listen "$server_port"
  else
    RUST_LOG="$RUST_LOG" "$BIN/grpc-server" --addr "127.0.0.1:$server_port" \
      --cert "$CERT" --key "$KEY" --workers "$workers" >"$OUT.server.log" 2>&1 &
    server_pid=$!
    wait_tcp_listen "$server_port"
  fi

  if [[ "$delay" != "0" ]]; then
    if [[ "$transport" == "quic" ]]; then
      proxy_listen=$((server_port + 1))
      "$BIN/delay-proxy" --mode udp --listen "127.0.0.1:$proxy_listen" \
        --upstream "127.0.0.1:$server_port" --delay-ms "$delay" >"$OUT.proxy.log" 2>&1 &
      proxy_pid=$!
      wait_udp_listen "$proxy_listen"
    else
      proxy_listen=$((server_port + 1))
      "$BIN/delay-proxy" --mode tcp --listen "127.0.0.1:$proxy_listen" \
        --upstream "127.0.0.1:$server_port" --delay-ms "$delay" >"$OUT.proxy.log" 2>&1 &
      proxy_pid=$!
      wait_tcp_listen "$proxy_listen"
    fi
    client_port="$proxy_listen"
  fi
  sleep 0.25

  local loadavg
  loadavg=$(awk '{printf "%s/%s/%s", $1, $2, $3}' /proc/loadavg)

  local extra=()
  if [[ "$transport" == "quic" ]]; then
    extra=( --tokio-threads "$QUIC_CLIENT_THREADS" --client-runtimes "$QUIC_CLIENT_RUNTIMES" )
  else
    extra=( --tokio-threads "$GRPC_CLIENT_THREADS" )
  fi

  local out rc=0
  set +e
  out=$(RUST_LOG="$RUST_LOG" timeout $((WARMUP + DURATION + 240)) \
    "$BIN/compare-client" --transport "$transport" --addr "127.0.0.1:$client_port" \
      --connections "$conns" --streams "$STREAMS" --depth "$depth" --size "$size" \
      --batch "$BATCH" --duration "$DURATION" --warmup "$WARMUP" --workers "$workers" \
      --delay-ms "$delay" --ca "$CA" --json "${extra[@]}" 2>&1)
  rc=$?
  set -e

  kill "$proxy_pid" 2>/dev/null || true
  kill "$server_pid" 2>/dev/null || true
  wait "$proxy_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true

  if [[ $rc -ne 0 ]]; then
    echo "  cell failed rc=$rc: $(printf '%s' "$out" | tail -2)" >&2
    return 1
  fi

  local json
  json=$(printf '%s\n' "$out" | grep '^{' | tail -1)
  if [[ -z "$json" ]]; then
    echo "  no JSON from cell: $(printf '%s' "$out" | tail -3)" >&2
    return 1
  fi

  printf '%s' "$json" | jq -r \
    --arg load "$loadavg" --argjson run "$run" --argjson delay "$delay" --argjson size "$size" \
    '[$load, $run, $delay, $size] as $x
     | [.transport, .workers, .connections, .pipeline_depth, $x[2], $x[3], $x[1], $x[0],
        .log_level, (.throughput_msg_s|floor), .rtt_p50_us, .rtt_p95_us, .rtt_p99_us,
        .rtt_p999_us, .e2e_p50_us, .e2e_p99_us, .queue_p50_us, .errors, .timeouts, .cutoff]
     | @csv' >> "$OUT"

  printf '  %-10s w=%-2s c=%-3s d=%-5s delay=%-2sms size=%-4s run=%s  %8s msg/s  p50=%7.0fus p99=%8.0fus  load=%s\n' \
    "$transport" "$workers" "$conns" "$depth" "$delay" "$size" "$run" \
    "$(printf '%s' "$json" | jq -r '.throughput_msg_s|floor')" \
    "$(printf '%s' "$json" | jq -r '.rtt_p50_us')" \
    "$(printf '%s' "$json" | jq -r '.rtt_p99_us')" "$loadavg"
}

# --------------------------------------------------------------------- matrix
if [[ "$QUIC_ONLY" == "1" ]]; then TRANSPORTS="quic"; else TRANSPORTS="grpc quic"; fi

for delay in $DELAYS; do
  for conns in $CONNS; do
    for depth in $DEPTHS; do
      for ((r = 1; r <= RUNS; r++)); do
        # Alternate transport order between repeats so a drifting shared host
        # cannot systematically favour whichever transport runs first.
        if (( r % 2 == 1 )); then ORDER="$TRANSPORTS"; else
          ORDER=$(printf '%s\n' $TRANSPORTS | tac | tr '\n' ' ')
        fi
        echo "--- run $r: ${conns}c depth ${depth} delay ${delay}ms order: $ORDER ---"
        for transport in $ORDER; do
          run_cell "$transport" "$WORKERS" "$conns" "$depth" "$delay" "$MSG_SIZE" "$r" || true
        done
      done
    done
  done
done

# ------------------------------------------------ QuicCrunch worker scaling
if [[ "$WORKER_SCALING" == "1" ]]; then
  for conns in 16 64; do
    for depth in 1 64 1800; do
      echo "--- quic 1w ${conns}c depth ${depth} delay 0ms ---"
      for ((r = 1; r <= RUNS; r++)); do
        run_cell quic 1 "$conns" "$depth" 0 "$MSG_SIZE" "$r" || true
      done
    done
  done
fi

# --------------------------------------------------- proxy overhead calibration
if [[ "$CALIBRATE" == "1" ]]; then
  echo
  echo "=== delay-proxy overhead calibration (16c depth 64, single run) ==="
  local_out="$OUT"
  CALRAW="${OUT%.csv}-calraw.csv"
  CALCSV="${OUT%.csv}-proxy.csv"
  OUT="$CALRAW"
  printf '%s\n' \
    "transport,workers,connections,depth,delay_ms,msg_size,run,loadavg,log_level,throughput_msg_s,rtt_p50_us,rtt_p95_us,rtt_p99_us,rtt_p999_us,e2e_p50_us,e2e_p99_us,queue_p50_us,errors,timeouts,cutoff" > "$OUT"
  printf 'transport,size,delay,path,rtt_p50_us,rtt_p99_us,throughput_msg_s\n' > "$CALCSV"
  for transport in $TRANSPORTS; do
    for size in 0 "$MSG_SIZE"; do
      for delay in 0 1 10; do
        # direct loopback
        run_cell "$transport" "$WORKERS" 16 64 0 "$size" 1 >/dev/null 2>&1 || true
        tail -1 "$OUT" | awk -F, -v t="$transport" -v s="$size" -v d="$delay" \
          '{gsub(/"/,""); printf "%s,%s,%s,direct,%s,%s,%s\n", t,s,d,$11,$13,$10}' \
          >> "$CALCSV"
        # through the proxy
        run_cell "$transport" "$WORKERS" 16 64 "$delay" "$size" 1 >/dev/null 2>&1 || true
        tail -1 "$OUT" | awk -F, -v t="$transport" -v s="$size" -v d="$delay" \
          '{gsub(/"/,""); printf "%s,%s,%s,proxy,%s,%s,%s\n", t,s,d,$11,$13,$10}' \
          >> "$CALCSV"
      done
    done
  done
  OUT="$local_out"
  echo "calibration rows: $CALCSV"
fi

echo
echo "results: $OUT"

SUMMARY="${OUT%.csv}-summary.md"
if command -v python3 >/dev/null 2>&1; then
  python3 scripts/comparison-summarize.py "$OUT" > "$SUMMARY"
  echo "summary: $SUMMARY"
fi