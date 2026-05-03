const SettingsModel = require("../models/Settings");
const { asyncHandler } = require("../utils/asyncHandler");

/**
 * GET /api/settings
 */
const getSettings = asyncHandler(async (req, res) => {
  const settings = await SettingsModel.get();
  res.set("Cache-Control", "public, max-age=30, stale-while-revalidate=60");
  res.json({ success: true, data: settings });
});

/**
 * PUT /api/settings
 * Update challengeStartTime and/or challengeEndTime.
 */
const updateSettings = asyncHandler(async (req, res) => {
  const { challengeStartTime, challengeEndTime } = req.body;

  const payload = {};

  if (challengeStartTime !== undefined) {
    payload.challengeStartTime = challengeStartTime ? new Date(challengeStartTime) : null;
    if (challengeStartTime && isNaN(payload.challengeStartTime)) {
      return res.status(400).json({ success: false, message: "Invalid challengeStartTime." });
    }
  }

  if (challengeEndTime !== undefined) {
    payload.challengeEndTime = challengeEndTime ? new Date(challengeEndTime) : null;
    if (challengeEndTime && isNaN(payload.challengeEndTime)) {
      return res.status(400).json({ success: false, message: "Invalid challengeEndTime." });
    }
  }

  if (
    payload.challengeStartTime &&
    payload.challengeEndTime &&
    payload.challengeStartTime >= payload.challengeEndTime
  ) {
    return res.status(400).json({
      success: false,
      message: "challengeEndTime must be after challengeStartTime.",
    });
  }

  const updated = await SettingsModel.upsert(payload);
  res.json({ success: true, data: updated });
});

/**
 * DELETE /api/settings/timers
 * Clear both start and end timers (game stays open indefinitely).
 */
const clearTimers = asyncHandler(async (req, res) => {
  await SettingsModel.clearTimers();
  res.json({ success: true, message: "Timers cleared. Game is now open indefinitely." });
});

module.exports = { getSettings, updateSettings, clearTimers };
