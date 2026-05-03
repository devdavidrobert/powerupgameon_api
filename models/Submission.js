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
 * @property {number[]} [answers]
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
   * Validate answers against Firestore questions, compute score server-side,
   * ignore any client-supplied prize/score. Uses a transaction for idempotency.
   *
   * @param {{ sessionId: string, fullName: string, normalizedName: string, answers: number[], userAgent?: string }} data
   * @returns {Promise<Submission>}
   */
  async create(data) {
    const QuestionModel = require("./Question");
    const db = getDb();
    const { sessionId, fullName, normalizedName, answers, userAgent } = data;

    const questions = await QuestionModel.findAll();
    if (!questions.length) {
      const err = new Error("No questions configured.");
      err.code = "NO_QUESTIONS";
      throw err;
    }

    if (!Array.isArray(answers) || answers.length !== questions.length) {
      const err = new Error("answers must match the number of questions.");
      err.code = "INVALID_ANSWERS_LENGTH";
      throw err;
    }

    let score = 0;
    for (let i = 0; i < questions.length; i++) {
      const q = questions[i];
      const ans = answers[i];
      if (!Number.isInteger(ans) || ans < 0 || ans >= q.options.length) {
        const err = new Error(`Invalid answer index for question ${i}.`);
        err.code = "INVALID_ANSWER_INDEX";
        throw err;
      }
      if (ans === q.correctIndex) score += 1;
    }

    const total = questions.length;
    if (score > total) {
      const err = new Error("Invalid score.");
      err.code = "INVALID_SCORE";
      throw err;
    }

    const percentage = Math.round((score / total) * 100);
    const prize = score === total ? "pending" : "Nothing";

    const submissionRef = db.collection(COLLECTION).doc(sessionId);
    const sessionRef = db.collection(SESSIONS_COLLECTION).doc(sessionId);

    return db.runTransaction(async (t) => {
      const [subSnap, sessionSnap] = await Promise.all([
        t.get(submissionRef),
        t.get(sessionRef),
      ]);

      if (!sessionSnap.exists) {
        const err = new Error("No registration found for this session.");
        err.code = "NO_SESSION";
        throw err;
      }

      if (subSnap.exists) {
        return { id: subSnap.id, ...subSnap.data() };
      }

      const now = new Date();
      const payload = {
        sessionId,
        fullName: fullName.toUpperCase(),
        normalizedName,
        score,
        total,
        percentage,
        prize,
        answers,
        userAgent: userAgent || "unknown",
        status: prize === "pending" ? "pending" : "completed",
        submittedAt: now,
      };

      t.set(submissionRef, payload);
      t.set(
        sessionRef,
        {
          hasPlayed: true,
          playedAt: now,
          score,
          percentage,
          prize,
          status: prize === "pending" ? "pending" : "completed",
        },
        { merge: true }
      );

      return { id: sessionId, ...payload };
    });
  },

  /**
   * Atomically assign wheel prize when submission is still "pending".
   * @param {string} sessionId
   * @param {string} prizeName
   * @returns {Promise<{ finalized: boolean, previousPrize?: string }>}
   */
  async finalizeSpinPrize(sessionId, prizeName) {
    const db = getDb();
    const now = new Date();
    const submissionRef = db.collection(COLLECTION).doc(sessionId);
    const sessionRef = db.collection(SESSIONS_COLLECTION).doc(sessionId);

    return db.runTransaction(async (t) => {
      const subSnap = await t.get(submissionRef);
      if (!subSnap.exists) {
        const err = new Error("Submission not found.");
        err.code = "NOT_FOUND";
        throw err;
      }

      const d = subSnap.data();
      if (d.prize && d.prize !== "pending") {
        return { finalized: false, previousPrize: d.prize };
      }

      t.update(submissionRef, {
        prize: prizeName,
        status: "completed",
        finalizedAt: now,
      });
      t.set(
        sessionRef,
        {
          prize: prizeName,
          status: "completed",
        },
        { merge: true }
      );

      return { finalized: true };
    });
  },

  /**
   * @param {string} id
   * @returns {Promise<void>}
   */
  async delete(id) {
    const db = getDb();
    const sub = await this.findById(id);
    const batch = db.batch();

    batch.delete(db.collection(COLLECTION).doc(id));

    if (sub) {
      const regSnap = await db.collection("registrations").doc(id).get();
      if (regSnap.exists) {
        const regData = regSnap.data();
        batch.delete(regSnap.ref);

        if (regData?.normalizedName) {
          batch.delete(db.collection("registrations").doc(`name_${regData.normalizedName}`));
        }

        if (regData?.ip && regData.ip !== "unknown") {
          const ipSnap = await db.collection("registrations").where("ip", "==", regData.ip).get();

          ipSnap.forEach((d) => {
            if (d.id !== id) {
              batch.delete(d.ref);
              batch.delete(db.collection(COLLECTION).doc(d.id));
              if (d.data()?.normalizedName) {
                batch.delete(db.collection("registrations").doc(`name_${d.data().normalizedName}`));
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
