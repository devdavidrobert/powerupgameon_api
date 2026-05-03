require("dotenv").config();

module.exports = {
  port: process.env.PORT || 4000,
  nodeEnv: process.env.NODE_ENV || "development",

  // Firebase
  firebaseProjectId: process.env.FIREBASE_PROJECT_ID,
  firebaseServiceAccountJson: process.env.FIREBASE_SERVICE_ACCOUNT_JSON,

  // CORS
  allowedOrigins: process.env.ALLOWED_ORIGINS?.split(",") || ["http://localhost:3000"],

  // Rate Limiting
  rateLimitWindowMs: 15 * 60 * 1000,
  rateLimitMax: 200,

  // Prize inventory cap
  realPrizeLimit: parseInt(process.env.REAL_PRIZE_LIMIT || "5", 10),

  // Session TTL (hours)
  sessionTtlHours: 12,
};
