const { getDb } = require("../config/firebase");

const COLLECTION = "prizes";

/**
 * @typedef {Object} Prize
 * @property {string}  id
 * @property {string}  name
 * @property {number}  order
 * @property {boolean} isRealPrize
 */

const PrizeModel = {
  /**
   * @returns {Promise<Prize[]>}
   */
  async findAll() {
    const snap = await getDb()
      .collection(COLLECTION)
      .orderBy("order")
      .get();

    return snap.docs.map((d) => ({
      id: d.id,
      isRealPrize: true, // backward-compat default
      ...d.data(),
    }));
  },

  /**
   * @param {string} id
   * @returns {Promise<Prize|null>}
   */
  async findById(id) {
    const doc = await getDb().collection(COLLECTION).doc(id).get();
    if (!doc.exists) return null;
    return { id: doc.id, isRealPrize: true, ...doc.data() };
  },

  /**
   * @param {{ name: string, order?: number, isRealPrize?: boolean }} data
   * @returns {Promise<Prize>}
   */
  async create(data) {
    const ref = await getDb().collection(COLLECTION).add({
      isRealPrize: true,
      ...data,
      createdAt: new Date(),
    });

    const doc = await ref.get();
    return { id: ref.id, ...doc.data() };
  },

  /**
   * @param {string} id
   * @param {Partial<Prize>} data
   * @returns {Promise<Prize>}
   */
  async update(id, data) {
    await getDb()
      .collection(COLLECTION)
      .doc(id)
      .update({ ...data, updatedAt: new Date() });

    return this.findById(id);
  },

  /**
   * @param {string} id
   * @returns {Promise<void>}
   */
  async delete(id) {
    await getDb().collection(COLLECTION).doc(id).delete();
  },
};

module.exports = PrizeModel;
