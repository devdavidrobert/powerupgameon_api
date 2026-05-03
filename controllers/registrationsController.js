const RegistrationModel = require("../models/Registration");
const SubmissionModel = require("../models/Submission");
const { asyncHandler } = require("../utils/asyncHandler");
const { normalizeName } = require("../utils/helpers");
const { log } = require("../utils/logger");
const { assertChallengeOpen } = require("../utils/challengeWindow");
const { getClientIp } = require("../utils/clientIp");
const { serializeDocData } = require("../utils/serializeFirestore");

const NAME_PART_PATTERN = /^[a-zA-Z0-9][a-zA-Z0-9'\- ]{0,49}$/;

function validateNamePart(value, label) {
  const t = typeof value === "string" ? value.trim() : "";
  if (!t) {
    return `${label} is required.`;
  }
  if (t.length > 50) {
    return `${label} must be at most 50 characters.`;
  }
  if (!NAME_PART_PATTERN.test(t)) {
    return `${label} may only contain letters, numbers, spaces, apostrophes, and hyphens.`;
  }
  return null;
}

/**
 * GET /api/registrations?limit=&cursor=
 * Paginated player list with completion status (batched submission lookups).
 */
const getAllRegistrations = asyncHandler(async (req, res) => {
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

  const { items, nextCursor, hasMore } = await RegistrationModel.findPlayerPage({ limit, cursor });
  const ids = items.map((r) => r.id);
  const completedIds = await SubmissionModel.idsThatExist(ids);

  const enriched = items.map((reg) => ({
    id: reg.id,
    ...serializeDocData(reg),
    status: completedIds.has(reg.id) ? "completed" : "incomplete",
  }));

  const nextCursorEncoded =
    hasMore && nextCursor
      ? Buffer.from(JSON.stringify(nextCursor), "utf8").toString("base64url")
      : null;

  res.json({
    success: true,
    data: enriched,
    nextCursor: nextCursorEncoded,
    hasMore,
  });
});

/**
 * POST /api/registrations
 */
const register = asyncHandler(async (req, res) => {
  const { firstName, lastName, sessionId, userAgent } = req.body;

  const fnErr = validateNamePart(firstName, "firstName");
  if (fnErr) {
    return res.status(400).json({ success: false, message: fnErr });
  }
  const lnErr = validateNamePart(lastName, "lastName");
  if (lnErr) {
    return res.status(400).json({ success: false, message: lnErr });
  }
  if (!sessionId) {
    return res.status(400).json({ success: false, message: "sessionId is required." });
  }

  try {
    await assertChallengeOpen();
  } catch (e) {
    if (e.code === "CHALLENGE_NOT_STARTED") {
      return res.status(403).json({ success: false, code: e.code, message: e.message });
    }
    if (e.code === "CHALLENGE_ENDED") {
      return res.status(403).json({ success: false, code: e.code, message: e.message });
    }
    throw e;
  }

  const fullName = `${firstName.trim()} ${lastName.trim()}`;
  const normalized = normalizeName(fullName);

  try {
    await RegistrationModel.register({
      sessionId,
      fullName,
      normalizedName: normalized,
      ip: getClientIp(req),
      userAgent: typeof userAgent === "string" ? userAgent : req.headers["user-agent"] || "unknown",
    });
  } catch (err) {
    if (err.code === "SESSION_COOLDOWN") {
      return res.status(409).json({
        success: false,
        code: "SESSION_COOLDOWN",
        message: err.message,
      });
    }
    if (err.code === "NAME_TAKEN") {
      return res.status(409).json({
        success: false,
        code: "NAME_TAKEN",
        message: `The name "${fullName}" has already been registered. One entry per person.`,
      });
    }
    throw err;
  }

  log("info", "registration_ok", { requestId: req.requestId, sessionId });

  res.status(201).json({
    success: true,
    message: "Registration successful.",
    data: { sessionId, fullName: fullName.toUpperCase() },
  });
});

/**
 * DELETE /api/registrations/:id
 */
const deleteRegistration = asyncHandler(async (req, res) => {
  const reg = await RegistrationModel.findById(req.params.id);
  if (!reg) return res.status(404).json({ success: false, message: "Registration not found." });

  await RegistrationModel.delete(req.params.id);
  res.json({ success: true, message: "Registration deleted. Player can now replay." });
});

module.exports = { getAllRegistrations, register, deleteRegistration };
