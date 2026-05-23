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
src/
├── main.rs                  # Server entry
├── lib.rs                   # Library crate (routes, models, middleware)
├── config.rs                # Environment configuration
├── routes.rs                # Axum router + middleware stack
├── controllers/             # HTTP handlers
├── models/                  # Firestore repositories
├── middleware/              # Auth, CSRF, rate limits, request context
├── services/                # Firestore + Firebase Auth clients
└── utils/                   # Spin tokens, helpers, serialization
```

---

## Authentication

Admin routes require a Firebase ID token in the `Authorization: Bearer …` header **and** admin authorization (custom claim or email allowlist).

Public routes use session IDs stored in Firestore; mutating `/api/*` routes require a valid `X-CSRF-Token` from `GET /api/csrf-token`.

---

## Tests

```bash
cargo test
```

Feature-level integration tests cover CSRF, spin tokens, helpers, auth bearer parsing, and admin allowlist logic.

---

## Health check

```
GET /health
GET /api/csrf-token
```

Default port: **4000** (`PORT` env var).
@