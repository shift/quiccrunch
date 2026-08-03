#!/usr/bin/env bash
# install-extension-deps.sh
#
# Installs npm dependencies for all package-style extensions.
# Safe to run multiple times (npm install is idempotent).

set -euo pipefail

EXTS_DIR=".pi/extensions"
PACKAGE_EXTENSIONS=(
  "model-failover"
  "engram-agents"
  "engram-orchestrator"
  "engram-tools"
  "engram-workflow"
)

echo "Installing extension dependencies..."
echo ""

for ext in "${PACKAGE_EXTENSIONS[@]}"; do
  dir="${EXTS_DIR}/${ext}"
  if [[ -f "${dir}/package.json" ]]; then
    echo "── ${ext} ──"
    (cd "$dir" && npm install --silent 2>&1 | tail -3)
    echo "  done"
  else
    echo "── ${ext}: skipped (no package.json at ${dir}/)"
  fi
done

echo ""
echo "All done."
