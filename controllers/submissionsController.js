const SubmissionModel = require("../models/Submission");
const PrizeModel = require("../models/Prize");
const { asyncHandler } = require("../utils/asyncHandler");
const { REAL_PRIZE_LIMIT } = require("../config/env");

/**
 * GET /api/submissions
 */
const getAllSubmissions = asyncHandler(async (req, res) => {
  const submissions = await SubmissionModel.findAll();
  res.json({ success: true, data: submissions, total: submissions.length });
});

/**
 * GET /api/submissions/:id
 */
const getSubmission = asyncHandler(async (req, res) => {
  const sub = await SubmissionModel.findById(req.params.id);
  if (!sub) return res.status(404).json({ success: false, message: "Submission not found." });
  res.json({ success: true, data: sub });
});

/**
 * POST /api/submissions
 * Create a submission (failed quiz or initiate spin with "pending" prize).
 */
const createSubmission = asyncHandler(async (req, res) => {
  const { sessionId, fullName, normalizedName, score, total, percentage, prize, userAgent } = req.body;

  if (!sessionId || fullName === undefined || score === undefined || total === undefined) {
    return res.status(400).json({
      success: false,
      message: "sessionId, fullName, score, and total are required.",
    });
  }

  const sub = await SubmissionModel.create({
    sessionId,
    fullName: fullName.toUpperCase(),
    normalizedName: normalizedName || fullName.trim().toLowerCase().replace(/\s+/g, " "),
    score: Number(score),
    total: Number(total),
    percentage: percentage !== undefined ? Number(percentage) : Math.round((score / total) * 100),
    prize: prize || "Nothing",
    userAgent: userAgent || req.headers["user-agent"] || "unknown",
  });

  res.status(201).json({ success: true, data: sub });
});

/**
 * POST /api/submissions/:sessionId/spin
 * Spin the wheel: select a prize respecting inventory limits.
 */
const spin = asyncHandler(async (req, res) => {
  const { sessionId } = req.params;

  const existing = await SubmissionModel.findById(sessionId);
  if (!existing) {
    return res.status(404).json({ success: false, message: "Submission not found for this session." });
  }
  if (existing.prize && existing.prize !== "pending") {
    return res.status(409).json({ success: false, message: "Prize already claimed for this session." });
  }

  const [prizes, prizeCounts] = await Promise.all([
    PrizeModel.findAll(),
    SubmissionModel.getPrizeCounts(),
  ]);

  if (!prizes.length) {
    return res.status(500).json({ success: false, message: "No prizes configured." });
  }

  // Filter available prizes
  const available = prizes.filter((p) => {
    if (!p.isRealPrize) return true; // consolation prizes always available
    const count = prizeCounts[p.name] || 0;
    return count < REAL_PRIZE_LIMIT;
  });

  const pool = available.length > 0 ? available : prizes.filter((p) => !p.isRealPrize);

  if (!pool.length) {
    return res.status(500).json({ success: false, message: "Prize inventory exhausted." });
  }

  const won = pool[Math.floor(Math.random() * pool.length)];

  await SubmissionModel.updatePrize(sessionId, won.name);

  res.json({
    success: true,
    data: {
      prize: won.name,
      isRealPrize: won.isRealPrize ?? true,
    },
  });
});

/**
 * DELETE /api/submissions/:id  (admin only)
 */
const deleteSubmission = asyncHandler(async (req, res) => {
  const sub = await SubmissionModel.findById(req.params.id);
  if (!sub) return res.status(404).json({ success: false, message: "Submission not found." });

  await SubmissionModel.delete(req.params.id);
  res.json({ success: true, message: "Submission and all linked records deleted." });
});

module.exports = { getAllSubmissions, getSubmission, createSubmission, spin, deleteSubmission };
