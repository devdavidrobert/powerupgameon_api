const QuestionModel = require("../models/Question");
const { asyncHandler } = require("../utils/asyncHandler");

/**
 * GET /api/questions
 */
const getAllQuestions = asyncHandler(async (req, res) => {
  const questions = await QuestionModel.findAll();
  res.json({ success: true, data: questions });
});

/**
 * GET /api/questions/:id
 */
const getQuestion = asyncHandler(async (req, res) => {
  const question = await QuestionModel.findById(req.params.id);
  if (!question) return res.status(404).json({ success: false, message: "Question not found." });
  res.json({ success: true, data: question });
});

/**
 * POST /api/questions
 */
const createQuestion = asyncHandler(async (req, res) => {
  const { text, options, correctIndex, order } = req.body;

  if (!text || !Array.isArray(options) || options.length < 2) {
    return res.status(400).json({
      success: false,
      message: "text and at least 2 options are required.",
    });
  }

  if (correctIndex === undefined || correctIndex < 0 || correctIndex >= options.length) {
    return res.status(400).json({
      success: false,
      message: "correctIndex must be a valid index within options array.",
    });
  }

  const allQuestions = await QuestionModel.findAll();
  const nextOrder = order ?? allQuestions.length + 1;

  const question = await QuestionModel.create({
    text: text.trim(),
    options: options.map((o) => o.trim()),
    correctIndex: Number(correctIndex),
    order: nextOrder,
  });

  res.status(201).json({ success: true, data: question });
});

/**
 * PUT /api/questions/:id
 */
const updateQuestion = asyncHandler(async (req, res) => {
  const existing = await QuestionModel.findById(req.params.id);
  if (!existing) return res.status(404).json({ success: false, message: "Question not found." });

  const { text, options, correctIndex, order } = req.body;
  const updates = {};

  if (text) updates.text = text.trim();
  if (Array.isArray(options)) updates.options = options.map((o) => o.trim());
  if (correctIndex !== undefined) updates.correctIndex = Number(correctIndex);
  if (order !== undefined) updates.order = Number(order);

  const updated = await QuestionModel.update(req.params.id, updates);
  res.json({ success: true, data: updated });
});

/**
 * DELETE /api/questions/:id
 */
const deleteQuestion = asyncHandler(async (req, res) => {
  const existing = await QuestionModel.findById(req.params.id);
  if (!existing) return res.status(404).json({ success: false, message: "Question not found." });

  await QuestionModel.delete(req.params.id);
  res.json({ success: true, message: "Question deleted." });
});

module.exports = { getAllQuestions, getQuestion, createQuestion, updateQuestion, deleteQuestion };
