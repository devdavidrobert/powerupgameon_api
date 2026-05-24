#!/usr/bin/env bash
# Print a Firebase ID token for admin API routes.
# Reads NEXT_PUBLIC_FIREBASE_API_KEY from ../powerupgameon/.env by default.

set -euo pipefail

FRONTEND_ENV="${FRONTEND_ENV:-$(dirname "$0")/../../powerupgameon/.env}"
EMAIL="${FIREBASE_EMAIL:-admin@steamenergy.com}"
PASSWORD="${FIREBASE_PASSWORD:-PowerUp}"

if [[ ! -f "$FRONTEND_ENV" ]]; then
  echo "Frontend .env not found at $FRONTEND_ENV" >&2
  exit 1
fi

API_KEY=$(grep '^NEXT_PUBLIC_FIREBASE_API_KEY=' "$FRONTEND_ENV" | cut -d= -f2- | tr -d '"')
if [[ -z "$API_KEY" ]]; then
  echo "NEXT_PUBLIC_FIREBASE_API_KEY not set in $FRONTEND_ENV" >&2
  exit 1
fi

curl -s \
  "https://identitytoolkit.googleapis.com/v1/accounts:signInWithPassword?key=${API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"${EMAIL}\",\"password\":\"${PASSWORD}\",\"returnSecureToken\":true}" \
  | python3 -c "
import sys, json
d = json.load(sys.stdin)
t = d.get('idToken')
if t:
    print(t)
else:
    sys.exit('Auth failed: ' + str(d.get('error', d)))
"
