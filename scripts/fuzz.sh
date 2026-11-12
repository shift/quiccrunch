#!/usr/bin/env bash
# Run the QuicCrunch wire-decoder fuzz targets for a bounded time.
#
# Requires a nightly toolchain and cargo-fuzz:
#   rustup toolchain install nightly
#   cargo install cargo-fuzz --locked
#
# Usage:
#   scripts/fuzz.sh                 # 60 seconds per target
#   FUZZ_SECONDS=120 scripts/fuzz.sh
#   FUZZ_TARGETS="raft_rpc_decode" scripts/fuzz.sh
set -euo pipefail

cd "$(dirname "$0")/.."

SECONDS_PER_TARGET="${FUZZ_SECONDS:-60}"
TARGETS="${FUZZ_TARGETS:-frame_codec messaging_decode raft_rpc_decode}"

if ! command -v cargo-fuzz >/dev/null 2>&1; then
    echo "error: cargo-fuzz is not installed (cargo install cargo-fuzz --locked)" >&2
    exit 1
fi

# Keep fuzz artifacts out of the repository root and out of /tmp.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/fuzz}"

# cargo-fuzz builds with an explicit --target, so the aesni dependency needs
# the x86 target features enabled explicitly.
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+aes,+ssse3,+sse2"

for target in $TARGETS; do
    echo "== fuzzing ${target} for ${SECONDS_PER_TARGET}s =="
    cargo fuzz run "${target}" -- \
        -max_total_time="${SECONDS_PER_TARGET}" \
        -print_final_stats=1
done
