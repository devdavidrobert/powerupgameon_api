const crypto = require("crypto");
const { apiCsrfSecret, nodeEnv } = require("../config/env");
const { log } = require("../utils/logger");

const TOKEN_TTL_MS = 60 * 60 * 1000;

function assertCsrfConfigured() {
  if (nodeEnv === "production" && !apiCsrfSecret) {
    throw new Error("API_CSRF_SECRET must be set in production.");
  }
}

/**
 * Mint a short-lived signed CSRF token (no cookie; safe for cross-origin API).
 */
function mintCsrfToken() {
  assertCsrfConfigured();
  const exp = Date.now() + TOKEN_TTL_MS;
  const payload = Buffer.from(JSON.stringify({ exp }), "utf8").toString("base64url");
  const sig = crypto.createHmac("sha256", apiCsrfSecret).update(payload).digest("base64url");
  return `${payload}.${sig}`;
}

function verifyCsrfToken(token) {
  if (!token || typeof token !== "string") return false;
  assertCsrfConfigured();
  const [payload, sig] = token.split(".");
  if (!payload || !sig) return false;
  const expected = crypto.createHmac("sha256", apiCsrfSecret).update(payload).digest("base64url");
  try {
    if (!crypto.timingSafeEqual(Buffer.from(sig), Buffer.from(expected))) return false;
  } catch {
    return false;
  }
  try {
    const { exp } = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
    if (typeof exp !== "number" || Date.now() > exp) return false;
    return true;
  } catch {
    return false;
  }
}

/**
 * Require X-CSRF-Token on mutating requests (after CORS preflight).
 */
function requireCsrfToken(req, res, next) {
  if (["GET", "HEAD", "OPTIONS"].includes(req.method)) {
    return next();
  }
  const token = req.headers["x-csrf-token"];
  if (!verifyCsrfToken(token)) {
    log("warn", "csrf_rejected", { requestId: req.requestId, path: req.path });
    return res.status(403).json({
      success: false,
      code: "CSRF_INVALID",
      message: "Invalid or missing CSRF token. Call GET /api/csrf-token first.",
    });
  }
  next();
}

module.exports = { mintCsrfToken, verifyCsrfToken, requireCsrfToken };
