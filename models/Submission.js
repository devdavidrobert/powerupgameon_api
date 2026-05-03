const { getDb } = require("../config/firebase");
const { FieldPath, Timestamp } = require("firebase-admin/firestore");

const COLLECTION = "submissions";
const SESSIONS_COLLECTION = "sessions";
const AGGREGATES_DOC = () => getDb().collection("system").doc("aggregates");

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
   * Batch existence check for submission doc ids (admin list enrichment).
   * @param {string[]} ids
   * @returns {Promise<Set<string>>}
   */
  async idsThatExist(ids) {
    if (!ids.length) return new Set();
    const db = getDb();
    const refs = ids.map((id) => db.collection(COLLECTION).doc(id));
    const snaps = await db.getAll(...refs);
    return new Set(snaps.filter((s) => s.exists).map((s) => s.id));
  },

  /**
   * Paginated submissions (newest first).
   * @param {{ limit?: number, cursor?: { submittedAt: number, id: string }|null }} opts
   */
  async findPage({ limit = 50, cursor = null }) {
    const db = getDb();
    const cap = Math.min(Math.max(Number(limit) || 50, 1), 200);
    let q = db
      .collection(COLLECTION)
      .orderBy("submittedAt", "desc")
      .orderBy(FieldPath.documentId(), "desc")
      .limit(cap);

    if (cursor && typeof cursor.submittedAt === "number" && cursor.id) {
      const ts = Timestamp.fromMillis(cursor.submittedAt);
      q = q.startAfter(ts, cursor.id);
    }

    const snap = await q.get();
    const items = snap.docs.map((d) => ({ id: d.id, ...d.data() }));
    const last = snap.docs[snap.docs.length - 1];
    const hasMore = snap.docs.length === cap;
    const nextCursor =
      last && hasMore
        ? { submittedAt: last.get("submittedAt")?.toMillis?.() ?? 0, id: last.id }
        : null;

    return { items, nextCursor, hasMore };
  },

  /**
   * Prize award counts from `system/aggregates` (O(1)); rebuild from scan if missing.
   * @returns {Promise<Record<string, number>>}
   */
  async getPrizeCounts() {
    const doc = await AGGREGATES_DOC().get();
    const counts = doc.exists ? doc.data().prizeAwardCounts : null;
    if (counts && typeof counts === "object" && Object.keys(counts).length > 0) {
      return counts;
    }
    return this.rebuildPrizeAwardCounts();
  },

  /**
   * Full scan — use after deploy or if aggregates drift (admin maintenance).
   * @returns {Promise<Record<string, number>>}
   */
  async rebuildPrizeAwardCounts() {
    const db = getDb();
    const snap = await db.collection(COLLECTION).get();
    const counts = {};

    snap.forEach((d) => {
      const { prize } = d.data();
      if (prize && prize !== "pending" && prize !== "Nothing") {
        counts[prize] = (counts[prize] || 0) + 1;
      }
    });

    await AGGREGATES_DOC().set(
      { prizeAwardCounts: counts, rebuiltAt: new Date() },
      { merge: true }
    );

    return counts;
  },

  /**
   * Validate answers against Firestore questions, compute score server-side,
   * ignore any client-supplied prize/score. Uses a transaction for idempotency.
   *
   * @param {{ sessionId: string, fullName: string, normalizedName: string, answers: number[], userAgent?: string, ip?: string }} data
   * @returns {Promise<Submission>}
   */
  async create(data) {
    const QuestionModel = require("./Question");
    const db = getDb();
    const { sessionId, fullName, normalizedName, answers, userAgent, ip } = data;

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
        ip: typeof ip === "string" ? ip : "unknown",
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
   * Increments `system/aggregates.prizeAwardCounts` when `isRealPrize` is true.
   * @param {string} sessionId
   * @param {string} prizeName
   * @param {boolean} isRealPrize
   * @returns {Promise<{ finalized: boolean, previousPrize?: string }>}
   */
  async finalizeSpinPrize(sessionId, prizeName, isRealPrize) {
    const db = getDb();
    const now = new Date();
    const submissionRef = db.collection(COLLECTION).doc(sessionId);
    const sessionRef = db.collection(SESSIONS_COLLECTION).doc(sessionId);
    const statsRef = AGGREGATES_DOC();

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

      if (isRealPrize) {
        const statsSnap = await t.get(statsRef);
        const prev = statsSnap.exists ? statsSnap.data().prizeAwardCounts || {} : {};
        const next = { ...prev, [prizeName]: (prev[prizeName] || 0) + 1 };
        t.set(statsRef, { prizeAwardCounts: next, updatedAt: now }, { merge: true });
      }

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
