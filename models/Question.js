const { getDb } = require("../config/firebase");

const COLLECTION = "questions";

const LIST_CACHE_TTL_MS = 45_000;
/** @type {{ at: number, rows: object[] } | null} */
let listCache = null;

/**
 * @typedef {Object} Question
 * @property {string}   id
 * @property {string}   text
 * @property {string[]} options
 * @property {number}   correctIndex
 * @property {number}   order
 */

const QuestionModel = {
  /** Call after admin create/update/delete so submissions see fresh questions quickly. */
  invalidateListCache() {
    listCache = null;
  },

  /**
   * Return all questions ordered by `order` field.
   * @returns {Promise<Question[]>}
   */
  async findAll() {
    const now = Date.now();
    if (listCache && now - listCache.at < LIST_CACHE_TTL_MS) {
      return listCache.rows;
    }

    const snap = await getDb()
      .collection(COLLECTION)
      .orderBy("order")
      .get();

    const rows = snap.docs.map((d) => ({ id: d.id, ...d.data() }));
    listCache = { at: now, rows };
    return rows;
  },

  /**
   * @param {string} id
   * @returns {Promise<Question|null>}
   */
  async findById(id) {
    const doc = await getDb().collection(COLLECTION).doc(id).get();
    return doc.exists ? { id: doc.id, ...doc.data() } : null;
  },

  /**
   * @param {{ text: string, options: string[], correctIndex: number, order?: number }} data
   * @returns {Promise<Question>}
   */
  async create(data) {
    const ref = await getDb()
      .collection(COLLECTION)
      .add({ ...data, createdAt: new Date() });

    const doc = await ref.get();
    return { id: ref.id, ...doc.data() };
  },

  /**
   * @param {string} id
   * @param {Partial<Question>} data
   * @returns {Promise<Question>}
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

module.exports = QuestionModel;
