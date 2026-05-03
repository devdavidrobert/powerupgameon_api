const RegistrationModel = require("../models/Registration");
const SubmissionModel = require("../models/Submission");
const { asyncHandler } = require("../utils/asyncHandler");
const { normalizeName } = require("../utils/helpers");

/**
 * GET /api/registrations
 * Returns all registrations cross-referenced with submission status.
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
 * Register a new player. Enforces name uniqueness + 12-hour device cooldown.
 */
const register = asyncHandler(async (req, res) => {
  const { firstName, lastName, sessionId, ip, userAgent } = req.body;

  if (!firstName?.trim() || !lastName?.trim()) {
    return res.status(400).json({ success: false, message: "firstName and lastName are required." });
  }
  if (!sessionId) {
    return res.status(400).json({ success: false, message: "sessionId is required." });
  }

  const fullName = `${firstName.trim()} ${lastName.trim()}`;
  const normalized = normalizeName(fullName);

  // Rule 1: Device cooldown
  const alreadyPlayed = await RegistrationModel.sessionHasPlayed(sessionId);
  if (alreadyPlayed) {
    return res.status(409).json({
      success: false,
      code: "SESSION_COOLDOWN",
      message: "You have already played. Please try again next time!",
    });
  }

  // Rule 2: Name uniqueness
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

  res.status(201).json({
    success: true,
    message: "Registration successful.",
    data: { sessionId, fullName: fullName.toUpperCase() },
  });
});

/**
 * DELETE /api/registrations/:id
 * Admin: completely wipe a player so they can replay.
 */
const deleteRegistration = asyncHandler(async (req, res) => {
  const reg = await RegistrationModel.findById(req.params.id);
  if (!reg) return res.status(404).json({ success: false, message: "Registration not found." });

  await RegistrationModel.delete(req.params.id);
  res.json({ success: true, message: "Registration deleted. Player can now replay." });
});

module.exports = { getAllRegistrations, register, deleteRegistration };
