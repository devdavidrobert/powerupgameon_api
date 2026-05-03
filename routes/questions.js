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

router.get("/admin/full", authenticate, getAllQuestionsAdmin);
router.get("/", getAllQuestions);
router.get("/:id", getQuestion);

router.post("/", authenticate, createQuestion);
router.put("/:id", authenticate, updateQuestion);
router.delete("/:id", authenticate, deleteQuestion);

module.exports = router;
