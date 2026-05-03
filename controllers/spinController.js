const crypto = require("crypto");
const SubmissionModel = require("../models/Submission");
const PrizeModel = require("../models/Prize");
const { asyncHandler } = require("../utils/asyncHandler");
const { REAL_PRIZE_LIMIT } = require("../config/env");
const { log } = require("../utils/logger");
const { assertChallengeOpen } = require("../utils/challengeWindow");
const { verifySpinToken } = require("../utils/spinToken");

const DEFAULT_ANIMATION_MS = 5000;

/**
 * POST /api/spin
 * Server-side prize selection with REAL_PRIZE_LIMIT; idempotent per session.
 * Body: { spinToken } — short-lived HMAC token minted on perfect-score submission.
 */
const spinWheel = asyncHandler(async (req, res) => {
  try {
    await assertChallengeOpen();
  } catch (e) {
    if (e.code === "CHALLENGE_NOT_STARTED" || e.code === "CHALLENGE_ENDED") {
      return res.status(403).json({ success: false, code: e.code, message: e.message });
    }
    throw e;
  }

  const spinToken = req.body && typeof req.body.spinToken === "string" ? req.body.spinToken : null;

  let sessionId;
  try {
    if (!spinToken) {
      return res.status(400).json({
        success: false,
        code: "SPIN_TOKEN_REQUIRED",
        message: "spinToken is required. Complete the quiz to receive a token.",
      });
    }
    sessionId = verifySpinToken(spinToken);
  } catch (e) {
    if (e.code === "SPIN_TOKEN_EXPIRED") {
      return res.status(401).json({ success: false, code: e.code, message: e.message });
    }
    if (e.code === "SPIN_TOKEN_INVALID") {
      return res.status(400).json({ success: false, code: e.code, message: e.message });
    }
    throw e;
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

  const pickIdx = crypto.randomInt(0, pool.length);
  const won = pool[pickIdx];
  const order = Number(won.order);

  const result = await SubmissionModel.finalizeSpinPrize(sessionId, won.name, !!won.isRealPrize);

  log("info", "spin_audit", {
    requestId: req.requestId,
    sessionIdPrefix: sessionId.slice(0, 10),
    prize: won.name,
    isRealPrize: !!won.isRealPrize,
    finalized: result.finalized,
  });

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
