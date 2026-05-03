const crypto = require("crypto");
const { spinTokenSecret, nodeEnv } = require("../config/env");

const TTL_MS = 20 * 60 * 1000;

function assertSecret() {
  if (nodeEnv === "production" && !process.env.SPIN_TOKEN_SECRET) {
    throw new Error("SPIN_TOKEN_SECRET must be set in production.");
  }
}

/**
 * @param {string} sessionId
 * @returns {string}
 */
function mintSpinToken(sessionId) {
  assertSecret();
  const exp = Date.now() + TTL_MS;
  const payload = Buffer.from(JSON.stringify({ sid: sessionId, exp }), "utf8").toString("base64url");
  const sig = crypto.createHmac("sha256", spinTokenSecret).update(payload).digest("base64url");
  return `${payload}.${sig}`;
}

/**
 * @param {string} token
 * @returns {string} sessionId
 */
function verifySpinToken(token) {
  assertSecret();
  if (!token || typeof token !== "string") {
    const err = new Error("spinToken is required.");
    err.code = "SPIN_TOKEN_INVALID";
    throw err;
  }
  const [payload, sig] = token.split(".");
  if (!payload || !sig) {
    const err = new Error("Invalid spin token.");
    err.code = "SPIN_TOKEN_INVALID";
    throw err;
  }
  const expected = crypto.createHmac("sha256", spinTokenSecret).update(payload).digest("base64url");
  try {
    if (!crypto.timingSafeEqual(Buffer.from(sig), Buffer.from(expected))) {
      const err = new Error("Invalid spin token.");
      err.code = "SPIN_TOKEN_INVALID";
      throw err;
    }
  } catch (e) {
    if (e.code === "SPIN_TOKEN_INVALID") throw e;
    const err = new Error("Invalid spin token.");
    err.code = "SPIN_TOKEN_INVALID";
    throw err;
  }
  let parsed;
  try {
    parsed = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
  } catch {
    const err = new Error("Invalid spin token.");
    err.code = "SPIN_TOKEN_INVALID";
    throw err;
  }
  if (typeof parsed.exp !== "number" || Date.now() > parsed.exp || typeof parsed.sid !== "string") {
    const err = new Error("Spin token expired.");
    err.code = "SPIN_TOKEN_EXPIRED";
    throw err;
  }
  return parsed.sid;
}

module.exports = { mintSpinToken, verifySpinToken, TTL_MS };
