const { getAuth } = require("../config/firebase");
const { asyncHandler } = require("../utils/asyncHandler");

/**
 * POST /api/auth/verify
 * Verify a Firebase ID token and return basic user info.
 * Used by the frontend to validate admin sessions server-side.
 */
const verifyToken = asyncHandler(async (req, res) => {
  const { idToken } = req.body;

  if (!idToken) {
    return res.status(400).json({ success: false, message: "idToken is required." });
  }

  const decoded = await getAuth().verifyIdToken(idToken);

  res.json({
    success: true,
    data: {
      uid: decoded.uid,
      email: decoded.email,
      emailVerified: decoded.email_verified,
    },
  });
});

/**
 * POST /api/auth/session
 * Exchange a Firebase ID token for a short-lived session cookie (optional pattern).
 * Useful if you move to server-rendered admin pages.
 */
const createSession = asyncHandler(async (req, res) => {
  const { idToken } = req.body;
  if (!idToken) {
    return res.status(400).json({ success: false, message: "idToken is required." });
  }

  // 5-day session cookie
  const expiresIn = 5 * 24 * 60 * 60 * 1000;
  const sessionCookie = await getAuth().createSessionCookie(idToken, { expiresIn });

  res.cookie("__session", sessionCookie, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "strict",
    maxAge: expiresIn,
  });

  res.json({ success: true, message: "Session created." });
});

module.exports = { verifyToken, createSession };
