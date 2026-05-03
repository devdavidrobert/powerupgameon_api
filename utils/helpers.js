/**
 * Normalize a player name for deduplication checks.
 * @param {string} name
 * @returns {string}
 */
const normalizeName = (name) =>
  name.trim().toLowerCase().replace(/\s+/g, " ");

/**
 * Fisher-Yates in-place shuffle.
 * @template T
 * @param {T[]} array
 * @returns {T[]}
 */
const fisherYatesShuffle = (array) => {
  for (let i = array.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [array[i], array[j]] = [array[j], array[i]];
  }
  return array;
};

/**
 * Convert a Firestore Timestamp or Date to an ISO string safely.
 * @param {any} value
 * @returns {string|null}
 */
const toISOString = (value) => {
  if (!value) return null;
  if (typeof value.toDate === "function") return value.toDate().toISOString();
  if (value instanceof Date) return value.toISOString();
  return null;
};

/**
 * Build a CSV string from an array of objects.
 * @param {Object[]} rows
 * @param {string[]} columns  Keys to include (in order)
 * @returns {string}
 */
const toCSV = (rows, columns) => {
  const header = columns.join(",");
  const body = rows
    .map((row) =>
      columns
        .map((col) => {
          const val = row[col] ?? "";
          return typeof val === "string" && val.includes(",")
            ? `"${val.replace(/"/g, '""')}"`
            : val;
        })
        .join(",")
    )
    .join("\n");
  return `${header}\n${body}`;
};

module.exports = { normalizeName, fisherYatesShuffle, toISOString, toCSV };
