const rateLimit = require("express-rate-limit");

/**
 * Strict limiter for the registration endpoint.
 * Prevents bots from rapidly bulk-registering names.
 */
const registrationLimiter = rateLimit({
  windowMs: 60 * 60 * 1000, // 1 hour
  max: 5,
  standardHeaders: true,
  legacyHeaders: false,
  message: {
    success: false,
    message: "Too many registration attempts from this IP. Please try again in an hour.",
  },
});

/**
 * Strict limiter for the spin endpoint.
 * One spin per session is enforced by logic, but this stops hammering.
 */
const spinLimiter = rateLimit({
  windowMs: 60 * 60 * 1000, // 1 hour
  max: 3,
  standardHeaders: true,
  legacyHeaders: false,
  message: {
    success: false,
    message: "Too many spin attempts. Please try again later.",
  },
});

module.exports = { registrationLimiter, spinLimiter };
