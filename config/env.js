require("dotenv").config();

const nodeEnv = process.env.NODE_ENV || "development";
const rawOrigins = process.env.ALLOWED_ORIGINS?.split(",").map((o) => o.trim()).filter(Boolean);
const allowedOrigins =
  rawOrigins.length > 0 ? rawOrigins : ["http://localhost:3000", "http://127.0.0.1:3000"];

const realPrizeLimit = parseInt(process.env.REAL_PRIZE_LIMIT || "5", 10);

module.exports = {
  port: process.env.PORT || 4000,
  nodeEnv,

  // Firebase
  firebaseProjectId: process.env.FIREBASE_PROJECT_ID,
  firebaseServiceAccountJson: process.env.FIREBASE_SERVICE_ACCOUNT_JSON,

  // CORS — never use * in production
  allowedOrigins,
  corsCredentials: true,

  // Rate Limiting
  rateLimitWindowMs: 15 * 60 * 1000,
  rateLimitMax: 200,

  // Prize inventory cap (alias for legacy imports)
  realPrizeLimit,
  REAL_PRIZE_LIMIT: realPrizeLimit,

  // Session TTL (hours)
  sessionTtlHours: 12,

  // CSRF: HMAC-signed tokens (set in production)
  apiCsrfSecret: process.env.API_CSRF_SECRET || (nodeEnv === "development" ? "dev-csrf-secret-change-me" : ""),

  // Optional Redis for rate limiting (REDIS_URL)
  redisUrl: process.env.REDIS_URL || "",
};
