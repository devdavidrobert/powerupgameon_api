const admin = require("firebase-admin");
const { log } = require("../utils/logger");

let db;

const initFirebase = () => {
  if (admin.apps.length > 0) return admin.app();

  const serviceAccount = process.env.FIREBASE_SERVICE_ACCOUNT_JSON
    ? JSON.parse(process.env.FIREBASE_SERVICE_ACCOUNT_JSON)
    : require("../serviceAccountKey.json"); // fallback for local dev

  admin.initializeApp({
    credential: admin.credential.cert(serviceAccount),
    projectId: process.env.FIREBASE_PROJECT_ID,
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
