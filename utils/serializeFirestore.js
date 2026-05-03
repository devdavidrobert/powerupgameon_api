/**
 * Convert common Firestore types to JSON-safe values for REST responses.
 * @param {FirebaseFirestore.DocumentData} data
 * @returns {Record<string, unknown>}
 */
function serializeDocData(data) {
  if (!data || typeof data !== "object") return {};
  const out = { ...data };
  for (const key of Object.keys(out)) {
    const v = out[key];
    if (v instanceof Date) {
      out[key] = v.toISOString();
    } else if (v && typeof v.toDate === "function") {
      out[key] = v.toDate().toISOString();
    }
  }
  return out;
}

module.exports = { serializeDocData };
