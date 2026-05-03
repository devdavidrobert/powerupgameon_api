const rateLimit = require("express-rate-limit");
const { redisUrl } = require("../config/env");
const { log } = require("../utils/logger");

function tryRedisStore() {
  if (!redisUrl) return null;
  try {
    const { RedisStore } = require("rate-limit-redis");
    const { createClient } = require("redis");
    const client = createClient({ url: redisUrl });
    client.connect().catch((err) => log("error", "redis_connect_failed", { err: String(err) }));
    return new RedisStore({
      sendCommand: (...args) => client.sendCommand(args),
    });
  } catch (err) {
    log("warn", "rate_limit_redis_unavailable", { err: String(err) });
    return null;
  }
}

const redisStore = tryRedisStore();

const { getClientIp } = require("../utils/clientIp");

const registrationLimiter = rateLimit({
  windowMs: 60 * 60 * 1000,
  max: 3,
  standardHeaders: true,
  legacyHeaders: false,
  keyPrefix: "rl_reg",
  ...(redisStore ? { store: redisStore } : {}),
  keyGenerator: (req) => getClientIp(req),
  message: {
    success: false,
    message: "Too many registration attempts from this IP. Please try again in an hour.",
  },
});

const submissionLimiter = rateLimit({
  windowMs: 15 * 60 * 1000,
  max: 30,
  standardHeaders: true,
  legacyHeaders: false,
  keyPrefix: "rl_sub",
  ...(redisStore ? { store: redisStore } : {}),
  keyGenerator: (req) => getClientIp(req),
  message: {
    success: false,
    message: "Too many submissions. Please try again later.",
  },
});

const spinLimiter = rateLimit({
  windowMs: 60 * 60 * 1000,
  max: 8,
  standardHeaders: true,
  legacyHeaders: false,
  keyPrefix: "rl_spin",
  ...(redisStore ? { store: redisStore } : {}),
  keyGenerator: (req) => {
    const body = req.body || {};
    const sid =
      typeof body.spinToken === "string"
        ? body.spinToken.slice(0, 24)
        : typeof body.sessionId === "string"
          ? body.sessionId
          : "na";
    return `${getClientIp(req)}:${sid}`;
  },
  message: {
    success: false,
    message: "Too many spin attempts. Please try again later.",
  },
});

module.exports = {
  registrationLimiter,
  submissionLimiter,
  spinLimiter,
  /** Shared Redis store for rate-limit-redis (null if REDIS_URL unset). */
  rateLimitRedisStore: redisStore,
};
