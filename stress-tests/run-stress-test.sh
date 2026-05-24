#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# PowerUpGameOn — Stress Test Runner
# Usage: bash run-stress-test.sh [profile]
#
# Profiles:
#   smoke    – quick sanity check (1 VU, 30 s)
#   load     – moderate load (ramp to 50 VUs over 5 min)
#   stress   – full stress test (all scenarios, default)
#   soak     – long-duration low-load soak (60 min)
#   spike    – sudden spike to 200 VUs
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

PROFILE="${1:-stress}"
SCRIPT="$(dirname "$0")/stress-test.js"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ── Environment ───────────────────────────────────────────────────────────────
# Override any of these via environment variables before running.
USE_DOCKER=false
if ! command -v k6 &>/dev/null; then
  if command -v docker &>/dev/null; then
    USE_DOCKER=true
  else
    echo "k6 not found and docker unavailable. Install k6: brew install k6" >&2
    exit 1
  fi
fi

if [[ "$USE_DOCKER" == true ]]; then
  API_URL="${API_URL:-http://host.docker.internal:4000/api}"
  BASE_URL="${BASE_URL:-http://host.docker.internal:3000}"
  API_URL="${API_URL//localhost/host.docker.internal}"
  API_URL="${API_URL//127.0.0.1/host.docker.internal}"
  BASE_URL="${BASE_URL//localhost/host.docker.internal}"
  BASE_URL="${BASE_URL//127.0.0.1/host.docker.internal}"
else
  API_URL="${API_URL:-http://localhost:4000/api}"
  BASE_URL="${BASE_URL:-http://localhost:3000}"
fi

CAMPAIGN_SLUG="${CAMPAIGN_SLUG:-default}"
ADMIN_TOKEN="${ADMIN_TOKEN:-}"            # Firebase ID token — leave blank to skip admin tests

if [[ "$USE_DOCKER" == true ]]; then
  K6=(
    docker run --rm
    -v "${SCRIPT_DIR}:/scripts"
    -w /scripts
    grafana/k6
  )
else
  K6=(k6)
fi

# ── Output dir ────────────────────────────────────────────────────────────────
RESULTS_DIR="./k6-results/$(date +%Y%m%d_%H%M%S)_${PROFILE}"
mkdir -p "$RESULTS_DIR"

echo ""
echo "════════════════════════════════════════════════════"
echo "  PowerUpGameOn Stress Test — Profile: ${PROFILE}"
echo "  API:      ${API_URL}"
echo "  Campaign: ${CAMPAIGN_SLUG}"
echo "  Results:  ${RESULTS_DIR}"
echo "════════════════════════════════════════════════════"
echo ""

# ── Common k6 flags ───────────────────────────────────────────────────────────
K6_FLAGS=(
  --out "json=${RESULTS_DIR}/raw.json"
  --summary-export "${RESULTS_DIR}/summary.json"
  --env "API_URL=${API_URL}"
  --env "CAMPAIGN_SLUG=${CAMPAIGN_SLUG}"
  --env "ADMIN_TOKEN=${ADMIN_TOKEN}"
  --env "BASE_URL=${BASE_URL}"
  --env "K6_PROFILE=${PROFILE}"
  --console-output "${RESULTS_DIR}/console.log"
)

# ── Profile-specific overrides ────────────────────────────────────────────────
case "$PROFILE" in
  smoke|load|stress|soak|spike|admin-lifecycle)
    "${K6[@]}" run "${K6_FLAGS[@]}" "$SCRIPT"
    ;;

  *)
    echo "Unknown profile: ${PROFILE}"
    echo "Available: smoke | load | stress | soak | spike | admin-lifecycle"
    exit 1
    ;;
esac

# ── Post-run summary ──────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════════════════"
echo "  Results written to: ${RESULTS_DIR}"
echo ""
echo "  Key files:"
echo "    raw.json     – full metric time-series (import into Grafana/InfluxDB)"
echo "    summary.json – pass/fail thresholds & aggregated stats"
echo "    console.log  – k6 stdout including group timings"
echo "════════════════════════════════════════════════════"
echo ""

# Optional: pretty-print threshold results if jq is available
if command -v jq &>/dev/null && [[ -f "${RESULTS_DIR}/summary.json" ]]; then
  echo "Threshold results:"
  jq '.metrics | to_entries[] | select(.value.thresholds != null)
      | {metric: .key, passed: (.value.thresholds | to_entries[] | .value.ok)}' \
    "${RESULTS_DIR}/summary.json" 2>/dev/null || true
fi
