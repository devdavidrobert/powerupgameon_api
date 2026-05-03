const SubmissionModel = require("../models/Submission");
const PrizeModel = require("../models/Prize");
const { asyncHandler } = require("../utils/asyncHandler");
const { REAL_PRIZE_LIMIT } = require("../config/env");
const { log } = require("../utils/logger");

const DEFAULT_ANIMATION_MS = 5000;

/**
 * POST /api/spin
 * Server-side prize selection with REAL_PRIZE_LIMIT; idempotent per session.
 */
const spinWheel = asyncHandler(async (req, res) => {
  const { sessionId } = req.body || {};

  if (!sessionId || typeof sessionId !== "string") {
    return res.status(400).json({
      success: false,
      message: "sessionId is required.",
    });
  }

  const existing = await SubmissionModel.findById(sessionId);
  if (!existing) {
    return res.status(404).json({
      success: false,
      message: "Submission not found for this session.",
    });
  }

  if (existing.prize && existing.prize !== "pending") {
    const prizes = await PrizeModel.findAll();
    const won = prizes.find((p) => p.name === existing.prize);
    const order = won ? Number(won.order) : 0;
    return res.json({
      success: true,
      data: {
        prize: { name: existing.prize, order, isRealPrize: won?.isRealPrize ?? false },
        animationDuration: DEFAULT_ANIMATION_MS,
        idempotent: true,
      },
    });
  }

  const [prizes, prizeCounts] = await Promise.all([
    PrizeModel.findAll(),
    SubmissionModel.getPrizeCounts(),
  ]);

  if (!prizes.length) {
    log("error", "spin_no_prizes", { requestId: req.requestId });
    return res.status(500).json({ success: false, message: "No prizes configured." });
  }

  const sorted = [...prizes].sort((a, b) => a.order - b.order);

  const available = sorted.filter((p) => {
    if (!p.isRealPrize) return true;
    const count = prizeCounts[p.name] || 0;
    return count < REAL_PRIZE_LIMIT;
  });

  const pool = available.length > 0 ? available : sorted.filter((p) => !p.isRealPrize);

  if (!pool.length) {
    return res.status(500).json({ success: false, message: "Prize inventory exhausted." });
  }

  const won = pool[Math.floor(Math.random() * pool.length)];
  const order = Number(won.order);

  const result = await SubmissionModel.finalizeSpinPrize(sessionId, won.name);

  if (!result.finalized && result.previousPrize) {
    const prev = sorted.find((p) => p.name === result.previousPrize);
    return res.json({
      success: true,
      data: {
        prize: {
          name: result.previousPrize,
          order: prev ? Number(prev.order) : order,
          isRealPrize: prev?.isRealPrize ?? false,
        },
        animationDuration: DEFAULT_ANIMATION_MS,
        idempotent: true,
      },
    });
  }

  res.json({
    success: true,
    data: {
      prize: { name: won.name, order, isRealPrize: won.isRealPrize ?? false },
      animationDuration: DEFAULT_ANIMATION_MS,
    },
  });
});

module.exports = { spinWheel };
