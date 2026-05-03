const SettingsModel = require("../models/Settings");

/**
 * Ensure the game is open per settings (server clock). Call from public mutating routes.
 * @throws {Error} code CHALLENGE_NOT_STARTED | CHALLENGE_ENDED
 */
async function assertChallengeOpen() {
  const { challengeStartTime, challengeEndTime } = await SettingsModel.get();
  const now = Date.now();

  if (challengeStartTime && now < challengeStartTime.getTime()) {
    const err = new Error("The challenge has not started yet.");
    err.code = "CHALLENGE_NOT_STARTED";
    throw err;
  }

  if (challengeEndTime && now > challengeEndTime.getTime()) {
    const err = new Error("The challenge has ended.");
    err.code = "CHALLENGE_ENDED";
    throw err;
  }
}

module.exports = { assertChallengeOpen };
