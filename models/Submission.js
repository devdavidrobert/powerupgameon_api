const { getDb } = require("../config/firebase");

const COLLECTION = "submissions";
const SESSIONS_COLLECTION = "sessions";

/**
 * @typedef {Object} Submission
 * @property {string}  id
 * @property {string}  fullName
 * @property {string}  normalizedName
 * @property {number}  score
 * @property {number}  total
 * @property {number}  percentage
 * @property {string}  prize
 * @property {string}  status   - "pending" | "completed"
 * @property {Date}    submittedAt
 * @property {Date}    [finalizedAt]
 */

const SubmissionModel = {
  /**
   * @returns {Promise<Submission[]>}
   */
  async findAll() {
    const snap = await getDb()
      .collection(COLLECTION)
      .orderBy("submittedAt", "desc")
      .get();

    return snap.docs.map((d) => ({ id: d.id, ...d.data() }));
  },

  /**
   * @param {string} id
   * @returns {Promise<Submission|null>}
   */
  async findById(id) {
    const doc = await getDb().collection(COLLECTION).doc(id).get();
    return doc.exists ? { id: doc.id, ...doc.data() } : null;
  },

  /**
   * Count submissions per prize name (for inventory tracking).
   * @returns {Promise<Record<string, number>>}
   */
  async getPrizeCounts() {
    const snap = await getDb().collection(COLLECTION).get();
    const counts = {};

    snap.forEach((doc) => {
      const { prize } = doc.data();
      if (prize && prize !== "pending" && prize !== "Nothing") {
        counts[prize] = (counts[prize] || 0) + 1;
      }
    });

    return counts;
  },

  /**
   * Create a new submission record. Idempotent: if sessionId already has a
   * submission it returns the existing one.
   * @param {{ sessionId: string, fullName: string, normalizedName: string, score: number, total: number, percentage: number, prize?: string, userAgent?: string }} data
   * @returns {Promise<Submission>}
   */
  async create(data) {
    const db = getDb();
    const { sessionId, ...rest } = data;

    const submissionRef = db.collection(COLLECTION).doc(sessionId);
    const sessionRef = db.collection(SESSIONS_COLLECTION).doc(sessionId);

    const [subSnap, sessionSnap] = await Promise.all([
      submissionRef.get(),
      sessionRef.get(),
    ]);

    if (!sessionSnap.exists) {
      throw new Error("No registration found for this session.");
    }

    // Idempotent — return existing record
    if (subSnap.exists) {
      return { id: subSnap.id, ...subSnap.data() };
    }

    const now = new Date();
    const payload = {
      ...rest,
      sessionId,
      status: rest.prize === "pending" ? "pending" : "completed",
      submittedAt: now,
    };

    await submissionRef.set(payload);

    // Mark session as played
    await sessionRef.set(
      { hasPlayed: true, playedAt: now, score: rest.score, percentage: rest.percentage },
      { merge: true }
    );

    return { id: sessionId, ...payload };
  },

  /**
   * Update the prize on an existing submission (called after wheel spin).
   * @param {string} sessionId
   * @param {string} prize
   * @returns {Promise<void>}
   */
  async updatePrize(sessionId, prize) {
    const db = getDb();
    const now = new Date();

    await Promise.all([
      db.collection(COLLECTION).doc(sessionId).update({
        prize,
        status: "completed",
        finalizedAt: now,
      }),
      db.collection(SESSIONS_COLLECTION).doc(sessionId).update({
        prize,
        status: "completed",
      }),
    ]);
  },

  /**
   * Hard delete a submission and its linked registration data.
   * @param {string} id
   * @returns {Promise<void>}
   */
  async delete(id) {
    const db = getDb();
    const sub = await this.findById(id);
    const batch = db.batch();

    // Delete submission
    batch.delete(db.collection(COLLECTION).doc(id));

    if (sub) {
      // Delete matching registration
      const regSnap = await db.collection("registrations").doc(id).get();
      if (regSnap.exists) {
        const regData = regSnap.data();
        batch.delete(regSnap.ref);

        if (regData?.normalizedName) {
          batch.delete(
            db.collection("registrations").doc(`name_${regData.normalizedName}`)
          );
        }

        // Delete all other registrations sharing the same IP
        if (regData?.ip && regData.ip !== "unknown") {
          const ipSnap = await db
            .collection("registrations")
            .where("ip", "==", regData.ip)
            .get();

          ipSnap.forEach((d) => {
            if (d.id !== id) {
              batch.delete(d.ref);
              batch.delete(db.collection(COLLECTION).doc(d.id));
              if (d.data()?.normalizedName) {
                batch.delete(
                  db.collection("registrations").doc(`name_${d.data().normalizedName}`)
                );
              }
            }
          });
        }
      }
    }

    await batch.commit();
  },
};

module.exports = SubmissionModel;
