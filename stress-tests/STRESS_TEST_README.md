# PowerUpGameOn — Stress Test Suite

## Prerequisites

```bash
# macOS
brew install k6

# Linux — see https://grafana.com/docs/k6/latest/set-up/install-k6/
```

---

## Quick Start

```bash
cd stress-tests

# For local stress runs, set `RATE_LIMIT_ENABLED=false` in the API `.env` and restart
# the server. Re-enable (`true` or remove the line) after testing.

# Smoke test (health + quiz + admin lifecycle, 1 iteration each)
API_URL=http://localhost:4000/api \
CAMPAIGN_SLUG=test3 \
ADMIN_TOKEN=$(./get-admin-token.sh) \
bash run-stress-test.sh smoke

# Full stress run (~9 min, read-only admin under load — no question mutations)
API_URL=http://localhost:4000/api \
CAMPAIGN_SLUG=test3 \
ADMIN_TOKEN=$(./get-admin-token.sh) \
bash run-stress-test.sh stress

# Destructive admin lifecycle only (create/update/delete questions & prizes)
bash run-stress-test.sh admin-lifecycle
```

---

## Scenario isolation

| Profile | Admin behaviour | Quiz load |
|---------|-----------------|-----------|
| `smoke` | Full lifecycle (1 iter) | 1 quiz flow |
| `stress` | **Read-only** lists/settings | Full ramp 0→60 VUs |
| `admin-lifecycle` | Create/update/delete only | None |

Admin question/prize mutations must **not** run concurrently with quiz flow — changing the question set mid-quiz causes `QUESTIONS_CHANGED` / submission failures.

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `API_URL` | `http://localhost:4000/api` | Base URL for the Rust API |
| `CAMPAIGN_SLUG` | `default` | Active campaign slug |
| `ADMIN_TOKEN` | _(empty)_ | Firebase ID token — omit to skip admin scenarios |
| `BASE_URL` | `http://localhost:3000` | Next.js frontend URL |

### API-side (`.env`)

| Variable | Default | Description |
|----------|---------|-------------|
| `RATE_LIMIT_ENABLED` | `true` | Set `false` for local stress runs |
| `GLOBAL_RATE_LIMIT_MAX` | `200` | Global requests per window per IP |
| `REGISTRATION_RATE_LIMIT_MAX` | `3` | Registrations per hour per IP |
| `SPIN_RATE_LIMIT_MAX` | `8` | Spins per hour per IP+session |

---

## Success criteria

| Metric | smoke | stress |
|--------|-------|--------|
| `quiz_completion_rate` | > 70% | > 80% |
| `spin_success_rate` | > 70% | > 95% |
| `csrf_fetch_errors` | < 10 | < 10 |
| `http_req_failed` | < 5% | < 8% |

---

## Profiles

| Profile | Duration | Description |
|---------|----------|-------------|
| `smoke` | ~1 min | Sanity check all paths |
| `stress` | ~9 min | Full load, 10 scenarios |
| `load` | ~5 min | Moderate ramp |
| `soak` | 60 min | Sustained background |
| `spike` | ~1.5 min | 0→200→0 VU spike |
| `admin-lifecycle` | ~30 s | Destructive admin CRUD only |

Results are written to `k6-results/` (gitignored).
