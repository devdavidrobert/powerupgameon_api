const { allowedAdminEmails } = require("../config/env");

/**
 * After authenticate(). Requires Firebase custom claim admin: true and/or email in ALLOWED_ADMIN_EMAILS.
 */
function requireAdmin(req, res, next) {
  const u = req.user;
  if (!u || !u.uid) {
    return res.status(401).json({
      success: false,
      message: "Unauthorized. A valid Bearer token is required.",
    });
  }

  if (u.admin === true) {
    return next();
  }

  const email = typeof u.email === "string" ? u.email.toLowerCase() : "";
  if (allowedAdminEmails.length > 0 && email && allowedAdminEmails.includes(email)) {
    return next();
  }

  return res.status(403).json({
    success: false,
    code: "FORBIDDEN_ADMIN",
    message: "Admin access required.",
  });
}

module.exports = { requireAdmin };
