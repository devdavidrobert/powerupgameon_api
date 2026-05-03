#!/usr/bin/env node
/**
 * Set Firebase Auth custom claims for admin UI / API access.
 * After running, the user must get a fresh ID token (sign out + sign in, or wait ~1 hour).
 *
 * Usage:
 *   node scripts/set-auth-admin-claim.js <uid> --grant
 *   node scripts/set-auth-admin-claim.js <uid> --revoke
 *
 * Requires FIREBASE_SERVICE_ACCOUNT_JSON in powerupgameon_api/.env
 */

require("dotenv").config();

const admin = require("firebase-admin");

function usage() {
  console.error(`Usage:
  node scripts/set-auth-admin-claim.js <uid> --grant    # sets { admin: true }
  node scripts/set-auth-admin-claim.js <uid> --revoke # removes admin claim

Then have the user sign out and sign in again so the ID token picks up new claims.
`);
}

async function main() {
  const args = process.argv.slice(2);
  const uid = args[0];
  const grant = args.includes("--grant");
  const revoke = args.includes("--revoke");

  if (!uid || (grant && revoke) || (!grant && !revoke)) {
    usage();
    process.exit(1);
  }

  const json = process.env.FIREBASE_SERVICE_ACCOUNT_JSON;
  if (!json?.trim()) {
    console.error("Missing FIREBASE_SERVICE_ACCOUNT_JSON in .env");
    process.exit(1);
  }

  let serviceAccount;
  try {
    serviceAccount = JSON.parse(json);
  } catch {
    console.error("FIREBASE_SERVICE_ACCOUNT_JSON must be valid JSON.");
    process.exit(1);
  }

  if (admin.apps.length === 0) {
    admin.initializeApp({
      credential: admin.credential.cert(serviceAccount),
      projectId: process.env.FIREBASE_PROJECT_ID || serviceAccount.project_id,
    });
  }

  const user = await admin.auth().getUser(uid);

  if (grant) {
    await admin.auth().setCustomUserClaims(uid, { ...user.customClaims, admin: true });
    console.log("Granted admin:true for UID", uid, "(email:", user.email + ")");
  } else {
    const next = { ...(user.customClaims || {}) };
    delete next.admin;
    const keys = Object.keys(next);
    if (keys.length === 0) {
      await admin.auth().setCustomUserClaims(uid, null);
    } else {
      await admin.auth().setCustomUserClaims(uid, next);
    }
    console.log("Revoked admin claim for UID", uid);
  }

  console.log("Tell the user to sign out and sign in again.");
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
