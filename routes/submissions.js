const router = require("express").Router();
const {
  getAllSubmissions,
  getSubmission,
  createSubmission,
  spin,
  deleteSubmission,
} = require("../controllers/submissionsController");
const { authenticate } = require("../middleware/authenticate");
const { spinLimiter } = require("../middleware/rateLimiters");

// Public
router.post("/", createSubmission);
router.post("/:sessionId/spin", spinLimiter, spin);

// Admin only
router.get("/", authenticate, getAllSubmissions);
router.get("/:id", authenticate, getSubmission);
router.delete("/:id", authenticate, deleteSubmission);

module.exports = router;
