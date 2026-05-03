const SubmissionModel = require("../models/Submission");
const { asyncHandler } = require("../utils/asyncHandler");
const { log } = require("../utils/logger");
const { assertChallengeOpen } = require("../utils/challengeWindow");
const { getClientIp } = require("../utils/clientIp");
const { mintSpinToken } = require("../utils/spinToken");
const { serializeDocData } = require("../utils/serializeFirestore");

/**
 * GET /api/submissions?limit=&cursor=
 */
const getAllSubmissions = asyncHandler(async (req, res) => {
  const limit = Math.min(parseInt(String(req.query.limit || "50"), 10) || 50, 200);
  let cursor = null;
  if (req.query.cursor) {
    try {
      const raw = Buffer.from(String(req.query.cursor), "base64url").toString("utf8");
      cursor = JSON.parse(raw);
    } catch {
      return res.status(400).json({ success: false, message: "Invalid cursor." });
    }
  }

  const { items, nextCursor, hasMore } = await SubmissionModel.findPage({ limit, cursor });
  const nextCursorEncoded =
    hasMore && nextCursor
      ? Buffer.from(JSON.stringify(nextCursor), "utf8").toString("base64url")
      : null;

  const data = items.map((row) => ({ id: row.id, ...serializeDocData(row) }));

  res.json({
    success: true,
    data,
    nextCursor: nextCursorEncoded,
    hasMore,
  });
});

/**
 * GET /api/submissions/:id
 */
const getSubmission = asyncHandler(async (req, res) => {
  const sub = await SubmissionModel.findById(req.params.id);
  if (!sub) return res.status(404).json({ success: false, message: "Submission not found." });
  res.json({ success: true, data: { id: sub.id, ...serializeDocData(sub) } });
});

/**
 * POST /api/submissions
 * Prize is never taken from the client — derived server-side from validated answers.
 */
const createSubmission = asyncHandler(async (req, res) => {
  const { sessionId, fullName, normalizedName, answers, userAgent } = req.body;

  if (!sessionId || typeof sessionId !== "string") {
    return res.status(400).json({
      success: false,
      message: "sessionId is required.",
    });
  }
  if (typeof fullName !== "string" || !fullName.trim()) {
    return res.status(400).json({
      success: false,
      message: "fullName is required.",
    });
  }
  if (!Array.isArray(answers)) {
    return res.status(400).json({
      success: false,
      message: "answers must be an array of option indices.",
    });
  }

  const normalized =
    typeof normalizedName === "string" && normalizedName.trim()
      ? normalizedName.trim().toLowerCase().replace(/\s+/g, " ")
      : fullName.trim().toLowerCase().replace(/\s+/g, " ");

  const sanitizedAnswers = [];
  for (let i = 0; i < answers.length; i++) {
    const raw = answers[i];
    const n = typeof raw === "number" ? raw : parseInt(String(raw), 10);
    if (!Number.isInteger(n)) {
      return res.status(400).json({
        success: false,
        message: `Answer at index ${i} must be an integer.`,
      });
    }
    sanitizedAnswers.push(n);
  }

  try {
    await assertChallengeOpen();
  } catch (e) {
    if (e.code === "CHALLENGE_NOT_STARTED" || e.code === "CHALLENGE_ENDED") {
      return res.status(403).json({ success: false, code: e.code, message: e.message });
    }
    throw e;
  }

  try {
    const ua =
      typeof userAgent === "string" && userAgent.length < 2000
        ? userAgent
        : req.headers["user-agent"] || "unknown";

    const sub = await SubmissionModel.create({
      sessionId,
      fullName,
      normalizedName: normalized,
      answers: sanitizedAnswers,
      userAgent: ua,
      ip: getClientIp(req),
    });

    const payload = { id: sub.id, ...serializeDocData(sub) };
    if (sub.prize === "pending" && sub.status === "pending") {
      try {
        payload.spinToken = mintSpinToken(sessionId);
      } catch (mintErr) {
        log("error", "spin_token_mint_failed", { requestId: req.requestId, err: String(mintErr) });
        throw mintErr;
      }
    }

    res.status(201).json({ success: true, data: payload });
  } catch (err) {
    const code = err.code;
    if (code === "NO_SESSION" || code === "INVALID_ANSWERS_LENGTH" || code === "INVALID_ANSWER_INDEX") {
      return res.status(400).json({
        success: false,
        message: err.message || "Validation failed.",
      });
    }
    if (code === "NO_QUESTIONS") {
      log("error", "submission_no_questions", { requestId: req.requestId });
      return res.status(500).json({
        success: false,
        message: "Game configuration error.",
      });
    }
    throw err;
  }
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

module.exports = { getAllSubmissions, getSubmission, createSubmission, deleteSubmission };
