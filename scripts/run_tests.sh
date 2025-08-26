#!/bin/bash

# Temporarily rename visualization modules to disable them
mv src/test_utils/plotting.rs src/test_utils/plotting.rs.bak || true
mv src/test_utils/power_viz.rs src/test_utils/power_viz.rs.bak || true

# Run the tests
nix-shell -p capnproto openssl pkg-config --command 'cargo test --test integration_transport --test flow_control_test --no-default-features --features="tls" --no-fail-fast -- --nocapture'

# Restore visualization modules
mv src/test_utils/plotting.rs.bak src/test_utils/plotting.rs || true
mv src/test_utils/power_viz.rs.bak src/test_utils/power_viz.rs || true