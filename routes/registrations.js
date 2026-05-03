const router = require("express").Router();
const {
  getAllRegistrations,
  register,
  deleteRegistration,
} = require("../controllers/registrationsController");
const { authenticate } = require("../middleware/authenticate");
const { registrationLimiter } = require("../middleware/rateLimiters");

// Public — called when player submits the identity form
router.post("/", registrationLimiter, register);

// Admin only
router.get("/", authenticate, getAllRegistrations);
router.delete("/:id", authenticate, deleteRegistration);

module.exports = router;
