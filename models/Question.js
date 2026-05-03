const { getDb } = require("../config/firebase");

const COLLECTION = "questions";

/**
 * @typedef {Object} Question
 * @property {string}   id
 * @property {string}   text
 * @property {string[]} options
 * @property {number}   correctIndex
 * @property {number}   order
 */

const QuestionModel = {
  /**
   * Return all questions ordered by `order` field.
   * @returns {Promise<Question[]>}
   */
  async findAll() {
    const snap = await getDb()
      .collection(COLLECTION)
      .orderBy("order")
      .get();

    return snap.docs.map((d) => ({ id: d.id, ...d.data() }));
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
