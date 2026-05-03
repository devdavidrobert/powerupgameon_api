const router = require("express").Router();
const {
  getAllQuestions,
  getQuestion,
  createQuestion,
  updateQuestion,
  deleteQuestion,
} = require("../controllers/questionsController");
const { authenticate } = require("../middleware/authenticate");

// Public — players need the question list to play
router.get("/", getAllQuestions);
router.get("/:id", getQuestion);

// Admin only
router.post("/", authenticate, createQuestion);
router.put("/:id", authenticate, updateQuestion);
router.delete("/:id", authenticate, deleteQuestion);

module.exports = router;
