const { getDb } = require("../config/firebase");
const { normalizeName } = require("../utils/helpers");

const COLLECTION = "registrations";
const SESSIONS_COLLECTION = "sessions";

/**
 * @typedef {Object} Registration
 * @property {string} id           - Fingerprint/session composite ID
 * @property {string} fullName
 * @property {string} normalizedName
 * @property {string} ip
 * @property {string} userAgent
 * @property {Date}   registeredAt
 */

const RegistrationModel = {
  /**
   * All registrations (excluding name-block sentinel docs).
   * @returns {Promise<Registration[]>}
   */
  async findAll() {
    const snap = await getDb()
      .collection(COLLECTION)
      .orderBy("registeredAt", "desc")
      .get();

    return snap.docs
      .map((d) => ({ id: d.id, ...d.data() }))
      .filter((r) => !r.id.startsWith("name_"));
  },

  /**
   * @param {string} id
   * @returns {Promise<Registration|null>}
   */
  async findById(id) {
    const doc = await getDb().collection(COLLECTION).doc(id).get();
    return doc.exists ? { id: doc.id, ...doc.data() } : null;
  },

  /**
   * Check if a normalized name is already blocked.
   * @param {string} normalized
   * @returns {Promise<boolean>}
   */
  async nameExists(normalized) {
    const doc = await getDb()
      .collection(COLLECTION)
      .doc(`name_${normalized}`)
      .get();
    return doc.exists;
  },

  /**
   * Check if a session has already played (within 12 h).
   * @param {string} sessionId
   * @returns {Promise<boolean>}
   */
  async sessionHasPlayed(sessionId) {
    const doc = await getDb()
      .collection(SESSIONS_COLLECTION)
      .doc(sessionId)
      .get();

    if (!doc.exists || !doc.data()?.hasPlayed) return false;

    const playedAt = doc.data()?.playedAt?.toDate();
    if (!playedAt) return false;

    const hoursAgo = (Date.now() - playedAt.getTime()) / (1000 * 60 * 60);
    return hoursAgo < 12;
  },

  /**
   * Register a player — creates session + name-block sentinel atomically.
   * @param {{ sessionId: string, fullName: string, normalizedName: string, ip?: string, userAgent?: string }} data
   * @returns {Promise<void>}
   */
  async register({ sessionId, fullName, normalizedName, ip = "unknown", userAgent = "unknown" }) {
    const db = getDb();
    const batch = db.batch();
    const now = new Date();

    // Session record
    batch.set(db.collection(SESSIONS_COLLECTION).doc(sessionId), {
      fullName: fullName.toUpperCase(),
      sessionId,
      hasPlayed: true,
      playedAt: now,
    });

    // Name-block sentinel
    batch.set(db.collection(COLLECTION).doc(`name_${normalizedName}`), {
      blocked: true,
      fullName: fullName.toUpperCase(),
      normalizedName,
      ip,
      userAgent,
      registeredAt: now,
    });

    await batch.commit();
  },

  /**
   * Delete a registration (+ name-block + optional submission) by ID.
   * @param {string} id
   * @returns {Promise<void>}
   */
  async delete(id) {
    const db = getDb();
    const reg = await this.findById(id);
    if (!reg) return;

    const batch = db.batch();
    batch.delete(db.collection(COLLECTION).doc(id));

    if (reg.normalizedName) {
      batch.delete(db.collection(COLLECTION).doc(`name_${reg.normalizedName}`));
    }

    await batch.commit();
  },
};

module.exports = RegistrationModel;
