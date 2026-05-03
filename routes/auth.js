const router = require("express").Router();
const { verifyToken, createSession } = require("../controllers/authController");

router.post("/verify", verifyToken);
router.post("/session", createSession);

module.exports = router;
