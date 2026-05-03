const { getDb } = require("../config/firebase");

const COLLECTION = "spin_tokens";

/**
 * TTL for consumed-token records in Firestore.
 * Must be >= spin token validity (20 min) to prevent replay after record cleanup.
 * We keep records for 2 hours to give Firestore TTL policy time to sweep them.
 */
const RECORD_TTL_MS = 2 * 60 * 60 * 1000;

/**
 * SpinToken model — tracks consumed spin token fingerprints to enforce single-use.
 *
 * Firestore collection: spin_tokens
 * Document ID:          SHA-256 hex fingerprint of the raw token
 * Fields:
 *   sessionId   {string}  - the session the token was minted for
 *   consumedAt  {Date}    - when it was first consumed
 *   expiresAt   {Date}    - TTL field; set a Firestore TTL policy on this field
 *                           so records self-clean after 2 hours
 */
const SpinTokenModel = {
  /**
   * Check whether a token fingerprint has already been consumed.
   * @param {string} fingerprint  SHA-256 hex of the raw spin token
   * @returns {Promise<boolean>}
   */
  async isConsumed(fingerprint) {
    const doc = await getDb().collection(COLLECTION).doc(fingerprint).get();
    return doc.exists;
  },

  /**
   * Atomically create a consumed-token record only if one does not already exist.
   * Uses Firestore create() which fails if the document exists — giving us
   * compare-and-swap semantics at the database level.
   *
   * @param {string} fingerprint  SHA-256 hex of the raw spin token
   * @param {string} sessionId    Session the token belongs to
   * @returns {Promise<boolean>}  true if this call was the one to consume it,
   *                              false if it was already consumed by another request
   */
  async consumeIfFresh(fingerprint, sessionId) {
    const now = new Date();
    const expiresAt = new Date(now.getTime() + RECORD_TTL_MS);

    try {
      await getDb().collection(COLLECTION).doc(fingerprint).create({
        sessionId,
        consumedAt: now,
        expiresAt,   // used by Firestore TTL policy — set it up in Firebase console
      });
      return true;
    } catch (err) {
      // Firestore throws ALREADY_EXISTS (code 6) when the doc already exists.
      if (err.code === 6 || (err.message && err.message.includes("ALREADY_EXISTS"))) {
        return false;
      }
      throw err;
    }
  },

  /**
   * Mark a token as consumed without the compare-and-swap guarantee.
   * Use this for idempotent paths where we know a prize is already assigned
   * and just want to prevent further probing.
   *
   * @param {string} fingerprint
   * @param {string} sessionId
   * @returns {Promise<void>}
   */
  async markConsumed(fingerprint, sessionId) {
    const now = new Date();
    const expiresAt = new Date(now.getTime() + RECORD_TTL_MS);
    await getDb()
      .collection(COLLECTION)
      .doc(fingerprint)
      .set({ sessionId, consumedAt: now, expiresAt }, { merge: true });
  },
};

module.exports = SpinTokenModel;