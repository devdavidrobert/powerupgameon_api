const crypto = require("crypto");
const SubmissionModel = require("../models/Submission");
const PrizeModel = require("../models/Prize");
const SpinTokenModel = require("../models/SpinToken");
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
 *
 * Security: spin tokens are single-use. Once consumed they are recorded in
 * `spin_tokens` collection and rejected on any subsequent attempt — even
 * within the 20-minute validity window.
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

  // --- ONE-TIME-USE ENFORCEMENT ---
  // Derive a stable token fingerprint (SHA-256 of the raw token string).
  // We store this rather than the raw token to avoid persisting a reusable secret.
  const tokenFingerprint = crypto.createHash("sha256").update(spinToken).digest("hex");

  const alreadyConsumed = await SpinTokenModel.isConsumed(tokenFingerprint);
  if (alreadyConsumed) {
    log("warn", "spin_token_replay_attempt", {
      requestId: req.requestId,
      sessionIdPrefix: sessionId.slice(0, 10),
    });
    return res.status(409).json({
      success: false,
      code: "SPIN_TOKEN_ALREADY_USED",
      message: "This spin token has already been used.",
    });
  }

  const existing = await SubmissionModel.findById(sessionId);
  if (!existing) {
    return res.status(404).json({
      success: false,
      message: "Submission not found for this session.",
    });
  }

  // Idempotency: if prize already assigned, return it (and consume the token
  // so replays still get rejected on next call).
  if (existing.prize && existing.prize !== "pending") {
    // Mark token consumed so this path cannot be probed repeatedly.
    await SpinTokenModel.markConsumed(tokenFingerprint, sessionId).catch(() => {});

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

  // --- MARK TOKEN CONSUMED BEFORE PRIZE ASSIGNMENT ---
  // Consume first so that concurrent requests with the same token both see it
  // as consumed after the first one wins.
  const consumed = await SpinTokenModel.consumeIfFresh(tokenFingerprint, sessionId);
  if (!consumed) {
    // Another concurrent request already consumed this token.
    return res.status(409).json({
      success: false,
      code: "SPIN_TOKEN_ALREADY_USED",
      message: "This spin token has already been used.",
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
    tokenFingerprint: tokenFingerprint.slice(0, 16),
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