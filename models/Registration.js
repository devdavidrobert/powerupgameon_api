const { getDb } = require("../config/firebase");
const { FieldPath, Timestamp } = require("firebase-admin/firestore");

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
   * All player registrations (`kind === "player"`), newest first.
   * @returns {Promise<Registration[]>}
   */
  async findAll() {
    const snap = await getDb()
      .collection(COLLECTION)
      .where("kind", "==", "player")
      .orderBy("registeredAt", "desc")
      .get();

    return snap.docs.map((d) => ({ id: d.id, ...d.data() }));
  },

  /**
   * Paginated player registrations for admin API.
   * @param {{ limit?: number, cursor?: { registeredAt: number, id: string }|null }} opts
   * @returns {Promise<{ items: Registration[], nextCursor: { registeredAt: number, id: string }|null, hasMore: boolean }>}
   */
  async findPlayerPage({ limit = 50, cursor = null }) {
    const db = getDb();
    const cap = Math.min(Math.max(Number(limit) || 50, 1), 200);
    let q = db
      .collection(COLLECTION)
      .where("kind", "==", "player")
      .orderBy("registeredAt", "desc")
      .orderBy(FieldPath.documentId(), "desc")
      .limit(cap);

    if (cursor && typeof cursor.registeredAt === "number" && cursor.id) {
      const ts = Timestamp.fromMillis(cursor.registeredAt);
      q = q.startAfter(ts, cursor.id);
    }

    const snap = await q.get();
    const items = snap.docs.map((d) => ({ id: d.id, ...d.data() }));
    const last = snap.docs[snap.docs.length - 1];
    const hasMore = snap.docs.length === cap;
    const nextCursor =
      last && hasMore
        ? { registeredAt: last.get("registeredAt")?.toMillis?.() ?? 0, id: last.id }
        : null;

    return { items, nextCursor, hasMore };
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
   * Register a player — transaction: name uniqueness, session cooldown, session + name lock + player profile.
   * @param {{ sessionId: string, fullName: string, normalizedName: string, ip?: string, userAgent?: string }} data
   * @returns {Promise<void>}
   */
  async register({ sessionId, fullName, normalizedName, ip = "unknown", userAgent = "unknown" }) {
    const db = getDb();
    const now = new Date();
    const nameRef = db.collection(COLLECTION).doc(`name_${normalizedName}`);
    const sessionRef = db.collection(SESSIONS_COLLECTION).doc(sessionId);
    const playerRef = db.collection(COLLECTION).doc(sessionId);

    await db.runTransaction(async (t) => {
      const [nameSnap, sessionSnap] = await Promise.all([t.get(nameRef), t.get(sessionRef)]);

      if (nameSnap.exists) {
        const err = new Error("The name has already been registered.");
        err.code = "NAME_TAKEN";
        throw err;
      }

      if (sessionSnap.exists && sessionSnap.data()?.hasPlayed) {
        const playedAt = sessionSnap.data()?.playedAt?.toDate?.() || null;
        if (playedAt) {
          const hoursAgo = (Date.now() - playedAt.getTime()) / (1000 * 60 * 60);
          if (hoursAgo < 12) {
            const err = new Error("You have already played. Please try again next time!");
            err.code = "SESSION_COOLDOWN";
            throw err;
          }
        } else {
          const err = new Error("You have already played. Please try again next time!");
          err.code = "SESSION_COOLDOWN";
          throw err;
        }
      }

      t.set(sessionRef, {
        fullName: fullName.toUpperCase(),
        sessionId,
        hasPlayed: true,
        playedAt: now,
      });

      t.set(nameRef, {
        kind: "name_lock",
        blocked: true,
        fullName: fullName.toUpperCase(),
        normalizedName,
        ip,
        userAgent,
        registeredAt: now,
      });

      t.set(playerRef, {
        kind: "player",
        sessionId,
        fullName: fullName.toUpperCase(),
        normalizedName,
        ip,
        userAgent,
        registeredAt: now,
      });
    });
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
