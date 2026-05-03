const router = require("express").Router();
const {
  getAllQuestions,
  getAllQuestionsAdmin,
  getQuestion,
  createQuestion,
  updateQuestion,
  deleteQuestion,
} = require("../controllers/questionsController");
const { authenticate } = require("../middleware/authenticate");
const { requireAdmin } = require("../middleware/requireAdmin");

router.get("/admin/full", authenticate, requireAdmin, getAllQuestionsAdmin);
router.get("/", getAllQuestions);
router.get("/:id", getQuestion);

router.post("/", authenticate, requireAdmin, createQuestion);
router.put("/:id", authenticate, requireAdmin, updateQuestion);
router.delete("/:id", authenticate, requireAdmin, deleteQuestion);

module.exports = router;
