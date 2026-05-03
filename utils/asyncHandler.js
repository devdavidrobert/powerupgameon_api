/**
 * Wraps an async route handler and forwards any thrown errors to Express's
 * error-handling middleware via next(err), eliminating boilerplate try/catch.
 *
 * @param {Function} fn  Async (req, res, next) handler
 * @returns {Function}
 */
const asyncHandler = (fn) => (req, res, next) =>
  Promise.resolve(fn(req, res, next)).catch(next);

module.exports = { asyncHandler };
