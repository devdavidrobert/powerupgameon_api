const router = require("express").Router();
const {
  getAllRegistrations,
  register,
  deleteRegistration,
} = require("../controllers/registrationsController");
const { authenticate } = require("../middleware/authenticate");
const { requireAdmin } = require("../middleware/requireAdmin");
const { registrationLimiter } = require("../middleware/rateLimiters");

// Public — called when player submits the identity form
router.post("/", registrationLimiter, register);

// Admin only
router.get("/", authenticate, requireAdmin, getAllRegistrations);
router.delete("/:id", authenticate, requireAdmin, deleteRegistration);

module.exports = router;
