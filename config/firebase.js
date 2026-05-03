const admin = require("firebase-admin");
const { log } = require("../utils/logger");

const nodeEnv = process.env.NODE_ENV || "development";

let db;

const initFirebase = () => {
  if (admin.apps.length > 0) return admin.app();

  const json = process.env.FIREBASE_SERVICE_ACCOUNT_JSON;
  if (!json) {
    if (nodeEnv === "production") {
      throw new Error(
        "FIREBASE_SERVICE_ACCOUNT_JSON is required in production (JSON string). Do not use service account files in deployed environments."
      );
    }
    throw new Error(
      "Set FIREBASE_SERVICE_ACCOUNT_JSON to the full service account JSON as a single-line string for local development. The serviceAccountKey.json fallback has been removed for security."
    );
  }

  let serviceAccount;
  try {
    serviceAccount = JSON.parse(json);
  } catch (e) {
    throw new Error("FIREBASE_SERVICE_ACCOUNT_JSON must be valid JSON.");
  }

  admin.initializeApp({
    credential: admin.credential.cert(serviceAccount),
    projectId: process.env.FIREBASE_PROJECT_ID || serviceAccount.project_id,
  });

  db = admin.firestore();
  log("info", "firebase_admin_initialized", {});

  return admin.app();
};

const getDb = () => {
  if (!db) throw new Error("Firestore not initialized. Call initFirebase() first.");
  return db;
};

const getAuth = () => admin.auth();

initFirebase();

module.exports = { admin, getDb, getAuth };
