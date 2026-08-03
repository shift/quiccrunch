#!/usr/bin/env bash
# check-prerequisites.sh
#
# Verifies all prerequisites for the pi engram extensions are met.
# Exits 0 if everything is OK, 1 if anything is missing.

set -euo pipefail

PASS=0
WARN=0
FAIL=0

ok()   { echo "  ✓ $*"; ((PASS++)) || true; }
warn() { echo "  ⚠ $*"; ((WARN++)) || true; }
fail() { echo "  ✗ $*"; ((FAIL++)) || true; }

echo "=========================================="
echo "  Pi Engram Extensions — Prerequisites"
echo "=========================================="
echo ""

# ── Core tools ────────────────────────────────────────────────────────────────
echo "── Core tools ──"

if command -v pi &>/dev/null; then
  ok "pi  ($(pi --version 2>/dev/null | head -1 || echo 'version unknown'))"
else
  fail "pi not found in PATH — install from https://github.com/badlogic/pi"
fi

if command -v node &>/dev/null; then
  ok "node  ($(node --version))"
else
  fail "node not found — required for TypeScript extensions"
fi

if command -v engram &>/dev/null; then
  ok "engram  ($(engram --version 2>/dev/null | head -1 || echo 'version unknown'))"
else
  fail "engram CLI not found in PATH — required for all engram-* extensions"
fi

if command -v python3 &>/dev/null; then
  ok "python3  ($(python3 --version 2>&1))"
else
  warn "python3 not found — used by setup scripts but not required at runtime"
fi

echo ""

# ── Extension directories ─────────────────────────────────────────────────────
echo "── Extensions (.pi/extensions/) ──"

EXTS_DIR=".pi/extensions"
EXPECTED_EXTENSIONS=(
  "model-failover"
  "engram-agents"
  "engram-orchestrator"
  "engram-tools"
  "engram-workflow"
)

for ext in "${EXPECTED_EXTENSIONS[@]}"; do
  if [[ -f "${EXTS_DIR}/${ext}/index.ts" ]]; then
    # Check node_modules
    if [[ -d "${EXTS_DIR}/${ext}/node_modules" ]]; then
      ok "${ext}  (deps installed)"
    else
      warn "${ext}  (index.ts present but node_modules missing — run: cd ${EXTS_DIR}/${ext} && npm install)"
    fi
  elif [[ -f "${EXTS_DIR}/${ext}.ts" ]]; then
    ok "${ext}  (single-file extension)"
  else
    fail "${ext}  NOT FOUND at ${EXTS_DIR}/${ext}/"
  fi
done

echo ""

# ── Config files ──────────────────────────────────────────────────────────────
echo "── Config files (.pi/) ──"

for cfg in ".pi/failover.json" ".pi/model-routing.json"; do
  if [[ -f "$cfg" ]]; then
    if python3 -c "import json; json.load(open('$cfg'))" 2>/dev/null; then
      ok "$cfg  (valid JSON)"
    else
      fail "$cfg  (INVALID JSON — needs fixing)"
    fi
  else
    warn "$cfg  not found — run setup-model-failover skill to create it"
  fi
done

echo ""

# ── Engram workspace ──────────────────────────────────────────────────────────
echo "── Engram workspace ──"

if [[ -d ".engram" ]]; then
  ok ".engram/  workspace found"
  if engram validate check &>/dev/null 2>&1; then
    ok "engram validate check passed"
  else
    warn "engram validate check had warnings — run 'engram validate check' for details"
  fi
else
  warn ".engram/ not found — run 'engram init' to initialise the workspace"
fi

echo ""

# ── Provider auth ─────────────────────────────────────────────────────────────
echo "── Provider auth (quick check) ──"

AUTH_FILE="${HOME}/.pi/agent/auth.json"
if [[ -f "$AUTH_FILE" ]]; then
  PROVIDERS=$(python3 -c "import json; d=json.load(open('$AUTH_FILE')); print(', '.join(d.keys()))" 2>/dev/null || echo "unreadable")
  ok "auth.json found — OAuth providers: ${PROVIDERS}"
else
  warn "No auth.json — no OAuth providers configured (github-copilot, etc.)"
fi

[[ -n "${ZAI_API_KEY:-}" ]]    && ok  "ZAI_API_KEY set (z.ai subscription)" \
                                || warn "ZAI_API_KEY not set"
[[ -n "${GROQ_API_KEY:-}" ]]   && ok  "GROQ_API_KEY set (groq free tier)" \
                                || warn "GROQ_API_KEY not set (optional — free tier)"
[[ -n "${GEMINI_API_KEY:-}" ]] && ok  "GEMINI_API_KEY set (google free tier)" \
                                || warn "GEMINI_API_KEY not set (optional — free tier)"

echo ""
echo "=========================================="
echo "  Summary: ${PASS} OK   ${WARN} warnings   ${FAIL} failures"
echo "=========================================="

[[ "$FAIL" -eq 0 ]]
