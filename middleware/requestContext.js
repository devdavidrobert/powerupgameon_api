const crypto = require("crypto");
const { log } = require("../utils/logger");

/**
 * Attach requestId for correlation; log completion with duration.
 */
function requestContext(req, res, next) {
  req.requestId = req.headers["x-request-id"] || crypto.randomUUID();
  res.setHeader("X-Request-Id", req.requestId);

  const start = Date.now();
  res.on("finish", () => {
    const durationMs = Date.now() - start;
    log(res.statusCode >= 500 ? "error" : "info", "http_request", {
      requestId: req.requestId,
      method: req.method,
      path: req.originalUrl,
      status: res.statusCode,
      durationMs,
    });
  });

  next();
}

module.exports = { requestContext };
