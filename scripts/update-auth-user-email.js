#!/usr/bin/env node
/**
 * Update a Firebase Auth user's email (Admin SDK).
 *
 * Usage (from powerupgameon_api, with .env loaded):
 *   node scripts/update-auth-user-email.js <uid> <new-email@example.com>
 *
 * Options:
 *   --no-verify   Leave emailVerified false after change (default: set true)
 *
 * Requires FIREBASE_SERVICE_ACCOUNT_JSON (and optionally FIREBASE_PROJECT_ID) in .env
 */

require("dotenv").config();

const admin = require("firebase-admin");

function usage() {
  console.error(`Usage: node scripts/update-auth-user-email.js <uid> <new-email> [--no-verify]

Example:
  node scripts/update-auth-user-email.js AbCdEf123 user@company.com
`);
}

async function main() {
  const args = process.argv.slice(2).filter((a) => a !== "--no-verify");
  const noVerify = process.argv.includes("--no-verify");

  const [uid, newEmail] = args;
  if (!uid || !newEmail) {
    usage();
    process.exit(1);
  }

  const json = process.env.FIREBASE_SERVICE_ACCOUNT_JSON;
  if (!json?.trim()) {
    console.error("Missing FIREBASE_SERVICE_ACCOUNT_JSON. Set it in powerupgameon_api/.env");
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

  const emailVerified = !noVerify;
  const user = await admin.auth().updateUser(uid, {
    email: newEmail.trim(),
    emailVerified,
  });

  console.log("Updated user:", {
    uid: user.uid,
    email: user.email,
    emailVerified: user.emailVerified,
  });
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
