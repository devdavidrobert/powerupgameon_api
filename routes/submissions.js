const router = require("express").Router();
const {
  getAllSubmissions,
  getSubmission,
  createSubmission,
  deleteSubmission,
} = require("../controllers/submissionsController");
const { authenticate } = require("../middleware/authenticate");
const { submissionLimiter } = require("../middleware/rateLimiters");

router.post("/", submissionLimiter, createSubmission);

router.get("/", authenticate, getAllSubmissions);
router.get("/:id", authenticate, getSubmission);
router.delete("/:id", authenticate, deleteSubmission);

module.exports = router;
