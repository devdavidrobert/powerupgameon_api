const { getDb } = require("../config/firebase");

const RAFFLES = "raffles";
const WINNERS = "raffle_winners";

/**
 * @typedef {Object} Raffle
 * @property {string} id
 * @property {string} name
 * @property {number} winnerCount
 * @property {Date}   createdAt
 */

/**
 * @typedef {Object} RaffleWinner
 * @property {string}  id
 * @property {string}  raffleId
 * @property {string}  raffleName
 * @property {string}  originalSubmissionId
 * @property {string}  fullName
 * @property {string}  prize
 * @property {number}  score
 * @property {number}  percentage
 * @property {boolean} giftReceived
 * @property {Date}    selectedAt
 */

const RaffleModel = {
  // ── Raffles ────────────────────────────────────────────────────────

  /** @returns {Promise<Raffle[]>} */
  async findAllRaffles() {
    const snap = await getDb()
      .collection(RAFFLES)
      .orderBy("createdAt", "desc")
      .get();

    return snap.docs.map((d) => ({
      id: d.id,
      ...d.data(),
      createdAt: d.data().createdAt?.toDate(),
    }));
  },

  /** @param {string} id @returns {Promise<Raffle|null>} */
  async findRaffleById(id) {
    const doc = await getDb().collection(RAFFLES).doc(id).get();
    return doc.exists
      ? { id: doc.id, ...doc.data(), createdAt: doc.data().createdAt?.toDate() }
      : null;
  },

  // ── Winners ────────────────────────────────────────────────────────

  /** @param {string} raffleId @returns {Promise<RaffleWinner[]>} */
  async findWinnersByRaffle(raffleId) {
    const snap = await getDb()
      .collection(WINNERS)
      .where("raffleId", "==", raffleId)
      .get();

    return snap.docs
      .map((d) => ({
        id: d.id,
        ...d.data(),
        selectedAt: d.data().selectedAt?.toDate(),
      }))
      .sort((a, b) => {
        if (a.giftReceived === b.giftReceived)
          return a.fullName.localeCompare(b.fullName);
        return a.giftReceived ? 1 : -1;
      });
  },

  /**
   * Create a raffle + its winners atomically.
   * @param {{ name: string, winners: Omit<RaffleWinner, 'id' | 'raffleId' | 'raffleName' | 'selectedAt' | 'giftReceived'>[] }} data
   * @returns {Promise<{ raffle: Raffle, winners: RaffleWinner[] }>}
   */
  async createRaffleWithWinners({ name, winners }) {
    const db = getDb();
    const batch = db.batch();
    const now = new Date();

    const raffleRef = db.collection(RAFFLES).doc();
    batch.set(raffleRef, {
      name,
      winnerCount: winners.length,
      createdAt: now,
    });

    const winnerRefs = winners.map((w) => {
      const ref = db.collection(WINNERS).doc();
      batch.set(ref, {
        ...w,
        raffleId: raffleRef.id,
        raffleName: name,
        giftReceived: false,
        selectedAt: now,
      });
      return ref;
    });

    await batch.commit();

    return {
      raffle: { id: raffleRef.id, name, winnerCount: winners.length, createdAt: now },
      winners: winnerRefs.map((ref, i) => ({
        id: ref.id,
        raffleId: raffleRef.id,
        raffleName: name,
        giftReceived: false,
        selectedAt: now,
        ...winners[i],
      })),
    };
  },

  /**
   * Toggle giftReceived on a raffle winner.
   * @param {string} winnerId
   * @param {boolean} giftReceived
   * @returns {Promise<void>}
   */
  async updateGiftReceived(winnerId, giftReceived) {
    await getDb()
      .collection(WINNERS)
      .doc(winnerId)
      .update({ giftReceived });
  },
};

module.exports = RaffleModel;
