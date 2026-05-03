/**
 * Centralized error handler.
 * Catches anything thrown inside asyncHandler or next(err) calls.
 */
const errorHandler = (err, req, res, next) => {
  const isDev = process.env.NODE_ENV !== "production";

  console.error(`[ERROR] ${req.method} ${req.originalUrl}`, err.message);
  if (isDev) console.error(err.stack);

  // Firebase / Firestore errors
  if (err.code === "permission-denied") {
    return res.status(403).json({ success: false, message: "Permission denied." });
  }
  if (err.code === "not-found") {
    return res.status(404).json({ success: false, message: "Resource not found." });
  }

  const statusCode = err.statusCode || err.status || 500;

  res.status(statusCode).json({
    success: false,
    message: err.message || "An unexpected error occurred.",
    ...(isDev && { stack: err.stack }),
  });
};

module.exports = { errorHandler };
