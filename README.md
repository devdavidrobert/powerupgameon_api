# Steam Energy API

Rust + Axum REST API for the Steam Energy Quiz & Spin Game, backed by Firebase Firestore.

---

## Setup

```bash
cp .env.example .env   # fill in your values
cargo run              # development / production server
```

Firebase credentials: set `FIREBASE_SERVICE_ACCOUNT_JSON` in `.env` (single-line JSON). Do not commit service account files; rotate any key that was ever exposed.

**Production operations**

- Set **`REDIS_URL`** so registration, submission, spin, and global rate limits share counters across all API instances. If it is unset in production, the process logs a warning and limiters fall back to in-memory stores.
- Set **`TRUST_PROXY=1`** (or `true`) when the API sits behind a reverse proxy or load balancer so client IPs and rate-limit keys use `X-Forwarded-For` correctly.
- Set **`API_CSRF_SECRET`** and **`SPIN_TOKEN_SECRET`** in production (required at startup).
- Optional **`SPIN_TOKEN_TTL_MINUTES`** (default `60`) controls how long a quiz-to-spin token remains valid.

**Vercel deployment**

- Set **`NODE_ENV=production`**, **`API_CSRF_SECRET`**, **`SPIN_TOKEN_SECRET`**, and **`FIREBASE_SERVICE_ACCOUNT_JSON`** on the API project. Missing secrets cause `FUNCTION_INVOCATION_FAILED` (browser shows a CORS error because no headers are returned).
- Set **`ALLOWED_ORIGINS=https://powerupgameon.vercel.app`** or **`FRONTEND_URL=https://powerupgameon.vercel.app`** (merged into CORS). Optional **`CORS_VERCEL_PROJECT=powerupgameon`** also allows preview deployment URLs.
- If **`REDIS_URL`** is set but unreachable, the API now falls back to in-memory rate limits instead of failing startup.

---

## Admin CLI tools

From `powerupgameon_api` with a populated `.env` (`FIREBASE_SERVICE_ACCOUNT_JSON`):

### Change a user’s Auth email

```bash
cargo run --bin update-auth-user-email -- <uid> <new-email@example.com>
```

- By default the script sets **`emailVerified: true`**. To leave the address unverified: add `--no-verify` after the email.
- The new email must not already belong to another user in the project.

### Grant or revoke admin custom claim (`admin: true`)

The Next.js `/api/admin/session` route and API `requireAdmin` accept either:

- **`admin: true` on the Firebase ID token** (recommended for production), or
- **`ALLOWED_ADMIN_EMAILS`** on the Next app (and API) matching the signed-in email.

```bash
cargo run --bin set-auth-admin-claim -- <uid> --grant
```

Then **sign out and sign in again** on the admin site so a new ID token includes the claim.

To remove: `cargo run --bin set-auth-admin-claim -- <uid> --revoke`.

---

## Project structure

```
api/
├── main.rs                  # Vercel serverless entry (Axum + vercel_runtime)
src/
├── main.rs                  # Long-running server entry (Render, Railway, local)
├── lib.rs                   # Library crate (routes, models, middleware)
├── config.rs                # Environment configuration
├── routes.rs                # Axum router + middleware stack
├── features/                # Campaign, locations, inventory (clean architecture)
│   ├── campaigns/
│   ├── locations/
│   └── inventory/
├── controllers/             # HTTP handlers
├── models/                  # Firestore repositories (campaign-scoped)
├── middleware/              # Auth, CSRF, rate limits, request context
├── services/                # Firestore + Firebase Auth clients
└── utils/                   # Spin tokens, helpers, serialization
```

---

## Multi-campaign API

All game data is scoped under a campaign, resolved by URL slug:

| Audience | Route |
|----------|-------|
| Admin | `GET/POST /api/campaigns` |
| Admin | `GET/PUT/DELETE /api/campaigns/{slug}` |
| Public | `GET /api/campaigns/{slug}/questions` |
| Public | `POST /api/campaigns/{slug}/registrations` (requires `lat`, `lng`) |
| Public | `POST /api/campaigns/{slug}/submissions` |
| Public | `POST /api/campaigns/{slug}/spin` |
| Public | `GET /api/campaigns/{slug}/settings` |
| Admin | `GET/POST /api/campaigns/{slug}/locations` |
| Admin | `GET/PUT /api/campaigns/{slug}/inventory` |

### Migration from single-tenant data

```bash
cargo run --bin migrate-to-campaigns -- --slug default --name "Legacy Campaign"
```

This creates a campaign, copies root collections into `campaigns/{id}/*` subcollections, adds a default geofence location, and seeds per-location inventory from `REAL_PRIZE_LIMIT` (migration CLI only) / `system/aggregates`.

---

## Authentication

Admin routes require a Firebase ID token in the `Authorization: Bearer …` header **and** admin authorization (custom claim or email allowlist).

Public routes use session IDs stored in Firestore; mutating `/api/*` routes require a valid `X-CSRF-Token` from `GET /api/csrf-token`.

---

## Tests

```bash
cargo test
```

Feature-level integration tests cover CSRF, spin tokens, campaign context, geo validation, inventory staggering, and auth middleware.

### Load / stress tests (k6)

See [`stress-tests/STRESS_TEST_README.md`](stress-tests/STRESS_TEST_README.md). Quick smoke run against a local API:

```bash
cd stress-tests
brew install k6   # once
API_URL=http://localhost:4000/api \
CAMPAIGN_SLUG=test3 \
ADMIN_TOKEN=$(./get-admin-token.sh) \
bash run-stress-test.sh smoke
```

---

## Health check

```
GET /health
GET /api/csrf-token
```

Default port: **4000** (`PORT` env var).

---

## Deploy to Vercel

This repo supports Vercel’s official [Rust + Axum runtime](https://vercel.com/docs/functions/runtimes/rust). All routes are served by a single serverless function (`api/main.rs`) with a catch-all rewrite.

### 1. Create a Vercel project

Connect the **`powerupgameon_api`** Git repository as its own Vercel project (separate from the Next.js frontend).

### 2. Environment variables

In the Vercel project → **Settings → Environment Variables**, add the same values as `.env`:

| Variable | Notes |
|----------|--------|
| `NODE_ENV` | `production` |
| `FIREBASE_SERVICE_ACCOUNT_JSON` | Single-line JSON string |
| `FIREBASE_PROJECT_ID` | e.g. `powerupgameon` |
| `ALLOWED_ORIGINS` | Comma-separated frontend URLs |
| `API_CSRF_SECRET` | Long random secret |
| `SPIN_TOKEN_SECRET` | Different long random secret |
| `TRUST_PROXY` | `1` |
| `REDIS_URL` | `redis://host:port` (scheme required) |
| `ALLOWED_ADMIN_EMAILS` | Comma-separated admin emails |
| `REAL_PRIZE_LIMIT` | Migration CLI only (`migrate-to-campaigns`), default `5` |
| `SPIN_TOKEN_TTL_MINUTES` | Spin token lifetime in minutes, default `60` |
| `IP_GEO_ENABLED` | `true`/`1` to cross-check GPS vs IP on registration (recommended in production) |
| `IP_GEO_MAX_DISTANCE_KM` | Max km between GPS and IP coords when both resolve; default `150` |
| `IP_GEO_API_URL` | Optional provider URL with `{ip}` placeholder; default uses ip-api.com |

### 3. Deploy

```bash
npx vercel            # first deploy (link project) — no global install needed
npx vercel --prod     # production deploy
```

If you prefer a global install and get `EACCES`, use `npx` as above instead of `sudo npm i -g vercel`.

Or push to the connected Git branch for automatic deploys.

### 4. Point the frontend at the new API

Set `NEXT_PUBLIC_API_URL` on the Next.js Vercel project to your API URL, e.g.:

```
https://your-api-project.vercel.app/api
```

### Local Vercel dev

```bash
vercel dev
```

Uses `.env` locally. The Axum app is cached per serverless instance after the first request (Firestore + Redis connections are reused).

### Notes

- **`maxDuration`** is set to 60s in `vercel.json` (Pro plan). Hobby tier caps at 10s.
- First request after idle may be slow (Rust compile + Firestore cold start).
- **`REDIS_URL` is strongly recommended** on Vercel so rate limits work across function instances.
- Long-running servers (Render, Railway, etc.) still use `cargo run` / `src/main.rs`.
@