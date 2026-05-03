const { nodeEnv } = require("../config/env");

/**
 * Structured logging — avoids raw console noise in production for non-errors.
 * @param {"info"|"warn"|"error"} level
 * @param {string} message
 * @param {Record<string, unknown>} [meta]
 */
function log(level, message, meta = {}) {
  const line = {
    level,
    message,
    time: new Date().toISOString(),
    ...meta,
  };
  const text = JSON.stringify(line);
  if (level === "error") {
    process.stderr.write(`${text}\n`);
    return;
  }
  if (nodeEnv === "production" && level === "info") {
    return;
  }
  process.stdout.write(`${text}\n`);
}

module.exports = { log };
