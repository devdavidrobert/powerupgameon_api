const router = require("express").Router();
const {
  getAllSubmissions,
  getSubmission,
  createSubmission,
  deleteSubmission,
} = require("../controllers/submissionsController");
const { authenticate } = require("../middleware/authenticate");
const { requireAdmin } = require("../middleware/requireAdmin");
const { submissionLimiter } = require("../middleware/rateLimiters");

router.post("/", submissionLimiter, createSubmission);

router.get("/", authenticate, requireAdmin, getAllSubmissions);
router.get("/:id", authenticate, requireAdmin, getSubmission);
router.delete("/:id", authenticate, requireAdmin, deleteSubmission);

module.exports = router;
