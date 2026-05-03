const RegistrationModel = require("../models/Registration");
const SubmissionModel = require("../models/Submission");
const { asyncHandler } = require("../utils/asyncHandler");
const { normalizeName } = require("../utils/helpers");
const { log } = require("../utils/logger");

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
 * GET /api/registrations
 */
const getAllRegistrations = asyncHandler(async (req, res) => {
  const [registrations, submissions] = await Promise.all([
    RegistrationModel.findAll(),
    SubmissionModel.findAll(),
  ]);

  const submissionIds = new Set(submissions.map((s) => s.id));

  const enriched = registrations.map((reg) => ({
    ...reg,
    status: submissionIds.has(reg.id) ? "completed" : "incomplete",
  }));

  res.json({ success: true, data: enriched });
});

/**
 * POST /api/registrations
 */
const register = asyncHandler(async (req, res) => {
  const { firstName, lastName, sessionId, ip, userAgent } = req.body;

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

  const fullName = `${firstName.trim()} ${lastName.trim()}`;
  const normalized = normalizeName(fullName);

  const alreadyPlayed = await RegistrationModel.sessionHasPlayed(sessionId);
  if (alreadyPlayed) {
    return res.status(409).json({
      success: false,
      code: "SESSION_COOLDOWN",
      message: "You have already played. Please try again next time!",
    });
  }

  const nameExists = await RegistrationModel.nameExists(normalized);
  if (nameExists) {
    return res.status(409).json({
      success: false,
      code: "NAME_TAKEN",
      message: `The name "${fullName}" has already been registered. One entry per person.`,
    });
  }

  await RegistrationModel.register({
    sessionId,
    fullName,
    normalizedName: normalized,
    ip: ip || req.ip || "unknown",
    userAgent: userAgent || req.headers["user-agent"] || "unknown",
  });

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
