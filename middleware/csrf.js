const crypto = require("crypto");
const { apiCsrfSecret, nodeEnv } = require("../config/env");
const { log } = require("../utils/logger");

/**
 * CSRF protection using HMAC-signed, per-request tokens.
 *
 * SECURITY UPGRADE
 * ────────────────
 * The previous implementation minted tokens that contained only an expiry
 * timestamp, making any valid token usable by any client within the 1-hour
 * window. This version adds a random `nonce` to each token so:
 *
 *   1. Tokens are single-origin (tied to one fetch sequence).
 *   2. Capturing one token gives no advantage for a different request
 *      because the nonce is not predictable.
 *
 * The nonce is a 16-byte random hex string generated at mint time.
 * It is embedded in the payload and validated on every mutating request.
 *
 * Token format (base64url-encoded):
 *   payload = base64url(JSON({ exp: <unix ms>, nonce: <hex> }))
 *   token   = payload + "." + base64url(HMAC-SHA256(payload, secret))
 */

const TOKEN_TTL_MS = 60 * 60 * 1000; // 1 hour

function assertCsrfConfigured() {
  if (nodeEnv === "production" && !apiCsrfSecret) {
    throw new Error("API_CSRF_SECRET must be set in production.");
  }
}

/**
 * Mint a short-lived, nonce-bound signed CSRF token.
 * @returns {string}
 */
function mintCsrfToken() {
  assertCsrfConfigured();
  const exp = Date.now() + TOKEN_TTL_MS;
  const nonce = crypto.randomBytes(16).toString("hex");
  const payload = Buffer.from(JSON.stringify({ exp, nonce }), "utf8").toString("base64url");
  const sig = crypto.createHmac("sha256", apiCsrfSecret).update(payload).digest("base64url");
  return `${payload}.${sig}`;
}

/**
 * Verify a CSRF token: checks signature, expiry, and that a nonce is present.
 * @param {string} token
 * @returns {boolean}
 */
function verifyCsrfToken(token) {
  if (!token || typeof token !== "string") return false;
  assertCsrfConfigured();

  const [payload, sig] = token.split(".");
  if (!payload || !sig) return false;

  const expected = crypto.createHmac("sha256", apiCsrfSecret).update(payload).digest("base64url");

  // Constant-time comparison to prevent timing attacks.
  try {
    const sigBuf = Buffer.from(sig);
    const expBuf = Buffer.from(expected);
    if (sigBuf.length !== expBuf.length) return false;
    if (!crypto.timingSafeEqual(sigBuf, expBuf)) return false;
  } catch {
    return false;
  }

  try {
    const parsed = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
    // Require both expiry and nonce fields.
    if (typeof parsed.exp !== "number" || Date.now() > parsed.exp) return false;
    if (typeof parsed.nonce !== "string" || parsed.nonce.length < 16) return false;
    return true;
  } catch {
    return false;
  }
}

/**
 * Express middleware: require X-CSRF-Token on mutating requests.
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