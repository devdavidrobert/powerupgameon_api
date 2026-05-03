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

Firebase credentials: either set `FIREBASE_SERVICE_ACCOUNT_JSON` in `.env`, or place `serviceAccountKey.json` in the project root.

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
│   ├── errorHandler.js      # Global error handler
│   ├── requestLogger.js     # Coloured request logs
│   └── rateLimiters.js      # Per-route rate limits
└── utils/
    ├── asyncHandler.js      # Wraps async handlers for Express
    └── helpers.js           # normalizeName, shuffle, CSV, etc.
```

---

## Authentication

Admin routes require a Firebase ID token in the `Authorization` header:

```
Authorization: Bearer <Firebase ID Token>
```

Obtain the token from Firebase client SDK: `auth.currentUser.getIdToken()`.

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
- `403` Forbidden — Firestore permission denied
- `404` Not Found
- `409` Conflict — name taken / session cooldown
- `422` Unprocessable — filter yields no results
- `429` Too Many Requests — rate limit hit
- `500` Internal Server Error
# powerupgameon_api
