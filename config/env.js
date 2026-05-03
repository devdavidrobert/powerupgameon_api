require("dotenv").config();

const nodeEnv = process.env.NODE_ENV || "development";
const rawOrigins = process.env.ALLOWED_ORIGINS?.split(",").map((o) => o.trim()).filter(Boolean);
const allowedOrigins =
  rawOrigins.length > 0 ? rawOrigins : ["http://localhost:3000", "http://127.0.0.1:3000"];

/** Lowercased emails; bootstrap allowlist. Prefer custom claim admin: true in production. */
const allowedAdminEmails = (process.env.ALLOWED_ADMIN_EMAILS || "")
  .split(",")
  .map((s) => s.trim().toLowerCase())
  .filter(Boolean);

const realPrizeLimit = parseInt(process.env.REAL_PRIZE_LIMIT || "5", 10);

/** Set TRUST_PROXY=1 when behind nginx, Cloud Run, etc., so req.ip / rate limits see the real client. */
const trustProxy = process.env.TRUST_PROXY === "1" || process.env.TRUST_PROXY === "true";

const spinTokenSecret =
  process.env.SPIN_TOKEN_SECRET ||
  (nodeEnv === "development" ? "dev-spin-token-secret-change-me" : "");

module.exports = {
  port: process.env.PORT || 4000,
  nodeEnv,

  // Firebase
  firebaseProjectId: process.env.FIREBASE_PROJECT_ID,
  firebaseServiceAccountJson: process.env.FIREBASE_SERVICE_ACCOUNT_JSON,

  // CORS — never use * in production
  allowedOrigins,
  corsCredentials: true,

  // Reverse proxy (nginx, load balancer) — required for accurate req.ip rate limits
  trustProxy,

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

  // Short-lived HMAC tokens for POST /spin (set in production)
  spinTokenSecret,

  // Optional Redis for rate limiting (REDIS_URL) — use in production for all limiters
  redisUrl: process.env.REDIS_URL || "",

  allowedAdminEmails,
};

if (nodeEnv === "production" && !process.env.REDIS_URL) {
  console.warn(
    "[steam-api] REDIS_URL is unset: rate limit counters are per-process only. Set REDIS_URL for multi-instance deployments."
  );
}
