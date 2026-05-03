const { getAuth } = require("../config/firebase");

/**
 * Verify Firebase ID token passed in the Authorization header.
 * Expected format: "Bearer <idToken>"
 */
const authenticate = async (req, res, next) => {
  const authHeader = req.headers.authorization;

  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    return res.status(401).json({
      success: false,
      message: "Unauthorized. A valid Bearer token is required.",
    });
  }

  const idToken = authHeader.split("Bearer ")[1];

  try {
    const decoded = await getAuth().verifyIdToken(idToken);
    req.user = {
      uid: decoded.uid,
      email: decoded.email,
    };
    next();
  } catch (err) {
    return res.status(401).json({
      success: false,
      message: "Invalid or expired token.",
    });
  }
};

module.exports = { authenticate };
