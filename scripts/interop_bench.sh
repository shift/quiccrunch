#!/usr/bin/env bash
set -euo pipefail

addr="127.0.0.1:0"
duration=${DURATION:-20}
streams_list=${STREAMS:-"1 32"}
sizes_list=${SIZES:-"1024 131072"}

run_case() {
  local server_feat=$1
  local client_feat=$2
  local streams=$3
  local size=$4

  # Start server
  srv_out=$(cargo run --no-default-features --features ${server_feat} --example interop_server -- --addr ${addr} --duration $((duration+5)) 2>/dev/null)
  bound=$(echo "$srv_out" | awk '/^BOUND /{print $2; exit}')
  if [[ -z "${bound}" ]]; then echo "Failed to get bound addr"; exit 1; fi

  res=$(cargo run --no-default-features --features ${client_feat} --example interop_client -- --addr ${bound} --streams ${streams} --size ${size} --duration ${duration} 2>/dev/null | awk '/^RESULT /{sub(/^RESULT /,""); print}')
  echo "${server_feat},${client_feat},${streams},${size},${res}"
}

printf "server,client,streams,size,json\n"
for s in ${streams_list}; do
  for z in ${sizes_list}; do
    run_case backend-quinn backend-quinn ${s} ${z}
    run_case backend-tquic backend-tquic ${s} ${z}
    run_case backend-quinn backend-tquic ${s} ${z}
    run_case backend-tquic backend-quinn ${s} ${z}
  done
done
