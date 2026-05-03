# Steam Energy API

Node.js + Express REST API for the Steam Energy Quiz & Spin Game, backed by Firebase Firestore.

---

## Setup

```bash
npm install
cp .env.example .env   # fill in your values
npm run dev            # development (nodemon)
npm start              # production
```

Firebase credentials: set `FIREBASE_SERVICE_ACCOUNT_JSON` in `.env` (single-line JSON). Do not commit service account files; rotate any key that was ever exposed.

**Production operations**

- Set **`REDIS_URL`** so registration, submission, spin, and global rate limits share counters across all API instances. If it is unset in production, the process logs a warning and limiters fall back to in-memory stores.
- Set **`TRUST_PROXY=1`** (or `true`) when the API sits behind a reverse proxy or load balancer so client IPs and rate-limit keys use `X-Forwarded-For` correctly.

### Change a user’s Auth email (Admin SDK)

From `powerupgameon_api` with a populated `.env` (`FIREBASE_SERVICE_ACCOUNT_JSON`):

```bash
npm run auth:update-email -- <uid> <new-email@example.com>
```

- By default the script sets **`emailVerified: true`**. To leave the address unverified: add `--no-verify` **after** the email.
- The new email must not already belong to another user in the project.

### Grant or revoke admin custom claim (`admin: true`)

The Next.js `/api/admin/session` route and Express `requireAdmin` accept either:

- **`admin: true` on the Firebase ID token** (recommended for production), or  
- **`ALLOWED_ADMIN_EMAILS`** on the Next app (and API) matching the signed-in email.

If you see **`Admin access required.`** after login, either add your email to `ALLOWED_ADMIN_EMAILS` in `powerupgameon/.env` (comma-separated, restart Next), **or** grant the claim:

```bash
cd powerupgameon_api
npm run auth:set-admin -- <uid> --grant
```

Then **sign out and sign in again** on the admin site so a new ID token includes the claim.

To remove: `npm run auth:set-admin -- <uid> --revoke`.

---

## Project Structure

```
steam-api/
├── app.js                   # Express app + server entry
├── config/
│   ├── firebase.js          # Firebase Admin SDK init
│   └── env.js               # Centralised env vars
├── routes/
│   ├── auth.js
│   ├── questions.js
│   ├── prizes.js
│   ├── registrations.js
│   ├── submissions.js
│   ├── settings.js
│   └── raffles.js
├── controllers/
│   ├── authController.js
│   ├── questionsController.js
│   ├── prizesController.js
│   ├── registrationsController.js
│   ├── submissionsController.js
│   ├── settingsController.js
│   └── rafflesController.js
├── models/
│   ├── Question.js
│   ├── Prize.js
│   ├── Registration.js
│   ├── Submission.js
│   ├── Settings.js
│   └── Raffle.js
├── middleware/
│   ├── authenticate.js      # Firebase ID token verification
│   ├── requireAdmin.js      # Admin custom claim / email allowlist
│   ├── errorHandler.js      # Global error handler
│   ├── requestLogger.js     # Coloured request logs
│   └── rateLimiters.js      # Per-route rate limits
└── utils/
    ├── asyncHandler.js      # Wraps async handlers for Express
    └── helpers.js           # normalizeName, shuffle, CSV, etc.
```

---

## Authentication

Admin routes require a Firebase ID token in the `Authorization` header **and** admin authorization:

```
Authorization: Bearer <Firebase ID Token>
```

Obtain the token from the Firebase client SDK: `auth.currentUser.getIdToken()`.

**Admin checks (production):** set the Firebase Auth custom claim `admin: true` for staff accounts. For bootstrap only, set `ALLOWED_ADMIN_EMAILS` in `.env` (comma-separated, same as the Next.js app). Requests that pass token verification but are not admins receive `403` with `code: "FORBIDDEN_ADMIN"`.

---

## API Reference

### Health
| Method | Path      | Auth | Description       |
|--------|-----------|------|-------------------|
| GET    | /health   | No   | Liveness check    |

### Auth
| Method | Path                | Auth | Description                            |
|--------|---------------------|------|----------------------------------------|
| POST   | /api/auth/verify    | No   | Verify a Firebase ID token             |
| POST   | /api/auth/session   | No   | Create a session cookie from ID token  |

### Questions
| Method | Path                 | Auth  | Description          |
|--------|----------------------|-------|----------------------|
| GET    | /api/questions       | No    | List all questions   |
| GET    | /api/questions/:id   | No    | Get one question     |
| POST   | /api/questions       | Admin | Create question      |
| PUT    | /api/questions/:id   | Admin | Update question      |
| DELETE | /api/questions/:id   | Admin | Delete question      |

### Prizes
| Method | Path              | Auth  | Description      |
|--------|-------------------|-------|------------------|
| GET    | /api/prizes       | No    | List prizes      |
| GET    | /api/prizes/:id   | No    | Get one prize    |
| POST   | /api/prizes       | Admin | Create prize     |
| PUT    | /api/prizes/:id   | Admin | Update prize     |
| DELETE | /api/prizes/:id   | Admin | Delete prize     |

### Registrations
| Method | Path                       | Auth  | Description                          |
|--------|----------------------------|-------|--------------------------------------|
| POST   | /api/registrations         | No    | Register a player (rate-limited)     |
| GET    | /api/registrations         | Admin | List all registrations with status   |
| DELETE | /api/registrations/:id     | Admin | Delete & reset a player              |

**POST body:**
```json
{
  "firstName": "John",
  "lastName": "Doe",
  "sessionId": "<uuid>",
  "ip": "optional",
  "userAgent": "optional"
}
```

### Submissions
| Method | Path                              | Auth  | Description                        |
|--------|-----------------------------------|-------|------------------------------------|
| POST   | /api/submissions                  | No    | Record a quiz result               |
| POST   | /api/submissions/:sessionId/spin  | No    | Spin wheel & assign prize          |
| GET    | /api/submissions                  | Admin | List all submissions               |
| GET    | /api/submissions/:id              | Admin | Get one submission                 |
| DELETE | /api/submissions/:id              | Admin | Hard-delete (wipes IP block too)   |

### Settings
| Method | Path                    | Auth  | Description                    |
|--------|-------------------------|-------|--------------------------------|
| GET    | /api/settings           | No    | Get current game schedule      |
| PUT    | /api/settings           | Admin | Update start/end times         |
| DELETE | /api/settings/timers    | Admin | Clear all timers               |

**PUT body:**
```json
{
  "challengeStartTime": "2025-12-25T09:00:00.000Z",
  "challengeEndTime":   "2025-12-25T17:00:00.000Z"
}
```

### Raffles
| Method | Path                              | Auth  | Description                     |
|--------|-----------------------------------|-------|---------------------------------|
| GET    | /api/raffles                      | Admin | List all raffle draws           |
| POST   | /api/raffles                      | Admin | Run a raffle, persist winners   |
| GET    | /api/raffles/:raffleId/winners    | Admin | Get winners for a raffle        |
| PATCH  | /api/raffles/winners/:winnerId    | Admin | Toggle giftReceived status      |

**POST body:**
```json
{
  "winnerCount": 3,
  "minScore": 8,
  "prizeWinnersOnly": false
}
```

---

## Error Responses

All errors follow this shape:
```json
{
  "success": false,
  "message": "Human-readable error message."
}
```

Common HTTP codes:
- `400` Bad Request — missing/invalid fields
- `401` Unauthorized — missing/invalid token
- `403` Forbidden — CSRF invalid or `FORBIDDEN_ADMIN` (not an admin user)
- `404` Not Found
- `409` Conflict — name taken / session cooldown
- `422` Unprocessable — filter yields no results
- `429` Too Many Requests — rate limit hit
- `500` Internal Server Error

---

## Tests

```bash
npm test
```

Runs Jest unit tests for `authenticate` and `requireAdmin` middleware.
