const { getDb } = require("../config/firebase");

const COLLECTION = "settings";
const DOC_ID = "general";

/**
 * @typedef {Object} GameSettings
 * @property {Date|null} challengeStartTime
 * @property {Date|null} challengeEndTime
 * @property {Date}      updatedAt
 */

const SettingsModel = {
  /**
   * @returns {Promise<GameSettings>}
   */
  async get() {
    const doc = await getDb().collection(COLLECTION).doc(DOC_ID).get();
    if (!doc.exists) {
      return { challengeStartTime: null, challengeEndTime: null };
    }
    const data = doc.data();
    return {
      challengeStartTime: data.challengeStartTime?.toDate() ?? null,
      challengeEndTime: data.challengeEndTime?.toDate() ?? null,
      updatedAt: data.updatedAt?.toDate() ?? null,
    };
  },

  /**
   * @param {{ challengeStartTime?: Date|null, challengeEndTime?: Date|null }} data
   * @returns {Promise<GameSettings>}
   */
  async upsert(data) {
    await getDb()
      .collection(COLLECTION)
      .doc(DOC_ID)
      .set({ ...data, updatedAt: new Date() }, { merge: true });

    return this.get();
  },

  /**
   * Remove both timers so the game stays open indefinitely.
   * @returns {Promise<void>}
   */
  async clearTimers() {
    await getDb()
      .collection(COLLECTION)
      .doc(DOC_ID)
      .set(
        { challengeStartTime: null, challengeEndTime: null, updatedAt: new Date() },
        { merge: true }
      );
  },
};

module.exports = SettingsModel;
