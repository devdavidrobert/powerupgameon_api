/**
 * PowerUpGameOn — Comprehensive Stress Test
 * Tool: k6 (https://k6.io)
 *
 * Usage:
 *   k6 run stress-test.js
 *   k6 run --env API_URL=https://your-api.vercel.app/api \
 *           --env CAMPAIGN_SLUG=nairobi-may-2026 \
 *           --env ADMIN_TOKEN=<firebase-id-token> \
 *           stress-test.js
 *
 * Scenarios covered:
 *   1. Health & CSRF baseline
 *   2. Public quiz flow  (register → submit → spin)
 *   3. Admin CRUD        (campaigns, questions, prizes, locations, inventory, settings)
 *   4. Raffle system     (create raffle, list, update gift status)
 *   5. Rate-limit probing
 *   6. Pagination / cursor stress
 *   7. Concurrent registration flood (duplicate/fairness enforcement)
 *   8. Spin token replay attack simulation
 *   9. Geofence boundary cases
 *  10. Long soak test    (steady background load)
 */

import http from "k6/http";
import { check, group, sleep, fail } from "k6";
import { Counter, Rate, Trend } from "k6/metrics";
import { SharedArray } from "k6/data";
import { uuidv4 } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";

// ─── Configuration ────────────────────────────────────────────────────────────

const API_URL     = __ENV.API_URL     || "http://localhost:4000/api";
const CAMPAIGN    = __ENV.CAMPAIGN_SLUG || "default";
const ADMIN_TOKEN = __ENV.ADMIN_TOKEN || ""; // Firebase ID token for admin calls
const BASE_URL    = __ENV.BASE_URL    || "http://localhost:3000"; // Next.js frontend
const PROFILE     = __ENV.K6_PROFILE  || "stress";

// Geofence coordinates — Nairobi CBD (inside zone)
const GEO_INSIDE  = { lat: -1.286389, lng: 36.817223 };
// Well outside any zone
const GEO_OUTSIDE = { lat: 51.507351, lng: -0.127758 }; // London

// ─── Custom Metrics ──────────────────────────────────────────────────────────

const csrfFetchErrors    = new Counter("csrf_fetch_errors");
const registrationErrors = new Counter("registration_errors");
const submissionErrors   = new Counter("submission_errors");
const spinErrors         = new Counter("spin_errors");
const adminErrors        = new Counter("admin_errors");
const duplicateBlocks    = new Counter("duplicate_registration_blocks");
const rateLimitHits      = new Counter("rate_limit_hits");
const geoBlocks          = new Counter("geo_blocks");

const registrationDuration = new Trend("registration_duration_ms", true);
const submissionDuration   = new Trend("submission_duration_ms", true);
const spinDuration         = new Trend("spin_duration_ms", true);
const adminCrudDuration    = new Trend("admin_crud_duration_ms", true);

const spinSuccessRate = new Rate("spin_success_rate");
const quizCompletionRate = new Rate("quiz_completion_rate");

// ─── Test Scenarios ──────────────────────────────────────────────────────────

const SCENARIOS = {

    // 1. Constant background traffic — health + CSRF + settings
    baseline_health: {
      executor: "constant-vus",
      vus: 5,
      duration: "3m",
      exec: "scenarioHealthAndCsrf",
      tags: { scenario: "baseline" },
    },

    // 2. Happy-path quiz flow at ramp-up load
    quiz_flow_ramp: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "30s", target: 10 },
        { duration: "1m",  target: 30 },
        { duration: "30s", target: 60 },
        { duration: "1m",  target: 60 },
        { duration: "30s", target: 0  },
      ],
      exec: "scenarioQuizFlow",
      tags: { scenario: "quiz_flow" },
      startTime: "5s",
    },

    // 3. Admin read-only (safe under load — no question/prize mutations)
    admin_readonly: {
      executor: "constant-vus",
      vus: 3,
      duration: "2m",
      exec: "scenarioAdminReadOnly",
      tags: { scenario: "admin_readonly" },
      startTime: "10s",
    },

    // 4. Raffle system
    raffle_system: {
      executor: "per-vu-iterations",
      vus: 2,
      iterations: 5,
      maxDuration: "2m",
      exec: "scenarioRaffle",
      tags: { scenario: "raffle" },
      startTime: "90s",
    },

    // 5. Rate-limit probing (registration endpoint)
    rate_limit_probe: {
      executor: "per-vu-iterations",
      vus: 5,
      iterations: 10,
      maxDuration: "1m",
      exec: "scenarioRateLimitProbe",
      tags: { scenario: "rate_limit" },
      startTime: "120s",
    },

    // 6. Pagination / cursor stress on submissions + registrations
    pagination_stress: {
      executor: "constant-vus",
      vus: 4,
      duration: "90s",
      exec: "scenarioPaginationStress",
      tags: { scenario: "pagination" },
      startTime: "30s",
    },

    // 7. Duplicate registration flood
    duplicate_flood: {
      executor: "per-vu-iterations",
      vus: 10,
      iterations: 3,
      maxDuration: "2m",
      exec: "scenarioDuplicateFlood",
      tags: { scenario: "duplicates" },
      startTime: "60s",
    },

    // 8. Spin token replay attack
    spin_replay_attack: {
      executor: "per-vu-iterations",
      vus: 5,
      iterations: 4,
      maxDuration: "2m",
      exec: "scenarioSpinReplay",
      tags: { scenario: "replay_attack" },
      startTime: "150s",
    },

    // 9. Geo boundary cases
    geo_boundary: {
      executor: "per-vu-iterations",
      vus: 5,
      iterations: 4,
      maxDuration: "90s",
      exec: "scenarioGeoBoundary",
      tags: { scenario: "geo" },
      startTime: "45s",
    },

    // 10. Long soak — sustained moderate load
    soak: {
      executor: "constant-vus",
      vus: 8,
      duration: "5m",
      exec: "scenarioSoak",
      tags: { scenario: "soak" },
      startTime: "3m30s",
    },
};

const PROFILE_SCENARIOS = {
  smoke: {
    baseline_health: {
      executor: "per-vu-iterations",
      vus: 1,
      iterations: 1,
      maxDuration: "30s",
      exec: "scenarioHealthAndCsrf",
      tags: { scenario: "baseline" },
    },
    quiz_flow_ramp: {
      executor: "per-vu-iterations",
      vus: 1,
      iterations: 1,
      maxDuration: "60s",
      exec: "scenarioQuizFlow",
      tags: { scenario: "quiz_flow" },
    },
    admin_lifecycle: {
      executor: "per-vu-iterations",
      vus: 1,
      iterations: 1,
      maxDuration: "60s",
      exec: "scenarioAdminLifecycle",
      tags: { scenario: "admin_lifecycle" },
    },
  },
  "admin-lifecycle": {
    admin_lifecycle: {
      executor: "per-vu-iterations",
      vus: 1,
      iterations: 1,
      maxDuration: "90s",
      exec: "scenarioAdminLifecycle",
      tags: { scenario: "admin_lifecycle" },
    },
  },
  load: {
    baseline_health: { ...SCENARIOS.baseline_health, vus: 10, duration: "5m" },
    quiz_flow_ramp: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "1m", target: 20 },
        { duration: "3m", target: 50 },
        { duration: "1m", target: 0 },
      ],
      exec: "scenarioQuizFlow",
      tags: { scenario: "quiz_flow" },
    },
  },
  soak: {
    soak: {
      executor: "constant-vus",
      vus: 10,
      duration: "60m",
      exec: "scenarioSoak",
      tags: { scenario: "soak" },
    },
  },
  spike: {
    quiz_flow_ramp: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "10s", target: 200 },
        { duration: "1m",  target: 200 },
        { duration: "10s", target: 0 },
      ],
      exec: "scenarioQuizFlow",
      tags: { scenario: "quiz_flow" },
    },
  },
};

const THRESHOLDS_BY_PROFILE = {
  smoke: {
    http_req_failed: ["rate<0.05"],
    csrf_fetch_errors: ["count<10"],
    spin_success_rate: ["rate>0.7"],
    quiz_completion_rate: ["rate>0.8"],
  },
  stress: {
    http_req_failed: ["rate<0.08"],
    registration_duration_ms: ["p(95)<3000"],
    submission_duration_ms: ["p(95)<3000"],
    spin_duration_ms: ["p(95)<5000"],
    admin_crud_duration_ms: ["p(95)<4000"],
    csrf_fetch_errors: ["count<10"],
    spin_success_rate: ["rate>0.95"],
    quiz_completion_rate: ["rate>0.8"],
  },
  "admin-lifecycle": {
    http_req_failed: ["rate<0.05"],
    csrf_fetch_errors: ["count<1"],
  },
};

export const options = {
  scenarios: PROFILE_SCENARIOS[PROFILE] || SCENARIOS,

  thresholds: THRESHOLDS_BY_PROFILE[PROFILE] || THRESHOLDS_BY_PROFILE.stress,
};

// ─── Shared Helpers ───────────────────────────────────────────────────────────

/**
 * Fetch a fresh CSRF token. Returns the token string or null on failure.
 */
function getCsrfToken() {
  const res = http.get(`${API_URL}/csrf-token`, { tags: { name: "csrf_token" } });
  if (res.status !== 200) {
    csrfFetchErrors.add(1);
    return null;
  }
  try {
    return res.json("data.csrfToken");
  } catch {
    csrfFetchErrors.add(1);
    return null;
  }
}

/**
 * Common JSON POST headers including CSRF + optional admin Bearer token.
 */
function headers(csrfToken, adminToken = null) {
  const h = {
    "Content-Type": "application/json",
    "X-CSRF-Token": csrfToken || "",
    "X-Request-Id": uuidv4(),
  };
  if (adminToken) h["Authorization"] = `Bearer ${adminToken}`;
  return h;
}

/**
 * Generate a unique player name that won't collide with other VUs.
 */
function randomPlayerName() {
  const id = uuidv4().replace(/-/g, "").slice(0, 8).toUpperCase();
  return { first: `VU${id}`, last: "TEST" };
}

/**
 * Generate random GPS coordinates within a 100 m radius of the base point.
 */
function jitter(base, maxDeltaDeg = 0.001) {
  return {
    lat: base.lat + (Math.random() - 0.5) * 2 * maxDeltaDeg,
    lng: base.lng + (Math.random() - 0.5) * 2 * maxDeltaDeg,
  };
}

/**
 * Fetch quiz questions for the campaign.
 * Returns array of questions (shuffles answer indices to simulate real answering).
 */
function fetchQuestions(csrfToken) {
  const res = http.get(
    `${API_URL}/campaigns/${CAMPAIGN}/questions`,
    { headers: headers(csrfToken), tags: { name: "get_questions" } }
  );
  if (!check(res, { "questions 200": (r) => r.status === 200 })) return null;
  try {
    return res.json("data");
  } catch {
    return null;
  }
}

/**
 * Build a "correct" answers array (all-zero — first option). For stress testing
 * we don't need real correctness; set ANSWER_MODE=correct for 100 % score tests.
 */
function buildAnswers(questions, allCorrect = false) {
  return questions.map((q, i) => (allCorrect ? 0 : i % q.options.length));
}

// ─── Scenario 1: Health & CSRF baseline ──────────────────────────────────────

export function scenarioHealthAndCsrf() {
  group("health_check", () => {
    const res = http.get(`${API_URL.replace("/api", "")}/health`, {
      tags: { name: "health" },
    });
    check(res, { "health ok": (r) => r.status === 200 });
  });

  sleep(0.5);

  group("csrf_token", () => {
    const tok = getCsrfToken();
    check(tok, { "csrf token received": (t) => t !== null && t.length > 10 });
  });

  sleep(0.5);

  group("public_settings", () => {
    const csrf = getCsrfToken();
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/settings`,
      { headers: headers(csrf), tags: { name: "public_settings" } }
    );
    check(res, { "settings 200 or 403": (r) => [200, 403].includes(r.status) });
  });

  sleep(1);
}

// ─── Scenario 2: Full Quiz Flow ───────────────────────────────────────────────

export function scenarioQuizFlow() {
  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }

  const { first, last } = randomPlayerName();
  const sessionId       = uuidv4();
  const { lat, lng }    = jitter(GEO_INSIDE);

  // ── Step 1: Register ────────────────────────────────────────────
  let regRes;
  group("registration", () => {
    const start = Date.now();
    regRes = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first,
        lastName:  last,
        sessionId,
        lat,
        lng,
        deviceId: uuidv4(),
        deviceFingerprint: {
          deviceId: uuidv4(),
          platform: "Linux x86_64",
          hardwareConcurrency: 4,
          screenWidth: 1920,
          screenHeight: 1080,
          timezone: "Africa/Nairobi",
          fingerprintHash: uuidv4().replace(/-/g, "").slice(0, 8),
        },
      }),
      { headers: headers(csrf), tags: { name: "register" } }
    );
    registrationDuration.add(Date.now() - start);

    const ok = check(regRes, {
      "registration 201": (r) => r.status === 201,
      "registration has sessionId": (r) => {
        try { return !!r.json("data.sessionId"); } catch { return false; }
      },
    });

    if (!ok) {
      registrationErrors.add(1);
      if (regRes.status === 409) duplicateBlocks.add(1);
      if (regRes.status === 429) rateLimitHits.add(1);
    }
  });

  if (!regRes || regRes.status !== 201) { sleep(1); return; }

  sleep(0.3);

  // ── Step 2: Fetch questions ─────────────────────────────────────
  const questions = fetchQuestions(csrf);
  if (!questions || questions.length === 0) { sleep(1); return; }

  // Simulate reading time (1-3 s per question, capped)
  sleep(Math.min(questions.length * 1.5, 8));

  const answers = buildAnswers(questions, /* allCorrect */ true); // force 100 % for spin access

  // ── Step 3: Submit ──────────────────────────────────────────────
  let subRes;
  group("submission", () => {
    const start = Date.now();
    subRes = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/submissions`,
      JSON.stringify({ sessionId, answers }),
      { headers: headers(csrf), tags: { name: "submit" } }
    );
    submissionDuration.add(Date.now() - start);

    const ok = check(subRes, {
      "submission 201 or 200": (r) => [200, 201].includes(r.status),
    });

    if (!ok) { submissionErrors.add(1); }
    quizCompletionRate.add(ok);
  });

  if (!subRes || ![200, 201].includes(subRes.status)) { sleep(1); return; }

  // Only proceed to spin if server returns a spinToken (100 % score)
  let spinToken;
  try { spinToken = subRes.json("data.spinToken"); } catch { /* */ }
  if (!spinToken) { sleep(1); return; }

  sleep(0.5);

  // ── Step 4: Spin ────────────────────────────────────────────────
  group("spin", () => {
    const start = Date.now();
    const spinRes = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/spin`,
      JSON.stringify({ spinToken }),
      { headers: headers(csrf), tags: { name: "spin" } }
    );
    spinDuration.add(Date.now() - start);

    const ok = check(spinRes, {
      "spin 200": (r) => r.status === 200,
      "spin has prize": (r) => {
        try { return !!r.json("data.prize.name"); } catch { return false; }
      },
      "spin campaign matches": (r) => {
        try { return r.json("data.campaignSlug") === CAMPAIGN; } catch { return false; }
      },
    });

    if (!ok) {
      spinErrors.add(1);
      if (spinRes.status === 429) rateLimitHits.add(1);
    }
    spinSuccessRate.add(ok);
  });

  sleep(1);
}

// ─── Scenario 3a: Admin read-only (safe under load) ────────────────────────────

export function scenarioAdminReadOnly() {
  if (!ADMIN_TOKEN) { sleep(5); return; }

  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }

  const h = headers(csrf, ADMIN_TOKEN);

  group("admin_campaign_list", () => {
    const start = Date.now();
    const res = http.get(`${API_URL}/campaigns`, { headers: h, tags: { name: "admin_list_campaigns" } });
    adminCrudDuration.add(Date.now() - start);
    const ok = check(res, { "campaigns list 200": (r) => r.status === 200 });
    if (!ok) adminErrors.add(1);
  });

  sleep(0.2);

  group("admin_questions_list", () => {
    const start = Date.now();
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/questions/admin/full`,
      { headers: h, tags: { name: "admin_list_questions" } }
    );
    adminCrudDuration.add(Date.now() - start);
    const ok = check(res, { "questions list 200": (r) => r.status === 200 });
    if (!ok) adminErrors.add(1);
  });

  sleep(0.2);

  group("admin_prizes_list", () => {
    const start = Date.now();
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/prizes/admin/full`,
      { headers: h, tags: { name: "admin_list_prizes" } }
    );
    adminCrudDuration.add(Date.now() - start);
    const ok = check(res, { "prizes list 200": (r) => r.status === 200 });
    if (!ok) adminErrors.add(1);
  });

  sleep(0.3);

  group("admin_registrations_list", () => {
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations?limit=50`,
      { headers: h, tags: { name: "admin_list_regs" } }
    );
    check(res, { "registrations 200": (r) => r.status === 200 });
  });

  sleep(0.3);

  group("admin_submissions_list", () => {
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/submissions?limit=50`,
      { headers: h, tags: { name: "admin_list_subs" } }
    );
    check(res, { "submissions 200": (r) => r.status === 200 });
  });

  sleep(0.3);

  group("admin_settings_read", () => {
    const getRes = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/settings/admin/full`,
      { headers: h, tags: { name: "admin_get_settings" } }
    );
    check(getRes, { "settings admin 200": (r) => r.status === 200 });
  });

  sleep(0.3);

  group("admin_locations_list", () => {
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/locations`,
      { headers: h, tags: { name: "admin_list_locations" } }
    );
    check(res, { "locations 200": (r) => r.status === 200 });
  });

  sleep(0.3);

  group("admin_inventory_list", () => {
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/inventory`,
      { headers: h, tags: { name: "admin_list_inventory" } }
    );
    check(res, { "inventory 200": (r) => r.status === 200 });
  });

  sleep(1);
}

// ─── Scenario 3b: Admin lifecycle (destructive — run isolated, not under quiz load) ──

export function scenarioAdminLifecycle() {
  if (!ADMIN_TOKEN) { sleep(5); return; }

  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }

  const h = headers(csrf, ADMIN_TOKEN);

  group("admin_question_lifecycle", () => {
    const createRes = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/questions`,
      JSON.stringify({
        text:         `Stress test question ${uuidv4().slice(0, 6)}?`,
        options:      ["Alpha", "Beta", "Gamma", "Delta"],
        correctIndex: 0,
        order:        999,
      }),
      { headers: h, tags: { name: "admin_create_question" } }
    );

    const created = check(createRes, {
      "question created 201": (r) => r.status === 201,
      "question create returned id": (r) => {
        try { return !!r.json("data.id"); } catch { return false; }
      },
    });
    if (!created) { adminErrors.add(1); return; }

    let qId;
    try { qId = createRes.json("data.id"); } catch { adminErrors.add(1); return; }
    if (!qId) { adminErrors.add(1); return; }

    sleep(0.2);

    const updateRes = http.put(
      `${API_URL}/campaigns/${CAMPAIGN}/questions/${qId}`,
      JSON.stringify({ text: "Updated stress question?", correctIndex: 1 }),
      { headers: h, tags: { name: "admin_update_question" } }
    );
    const updated = check(updateRes, { "question updated 200": (r) => r.status === 200 });
    if (!updated) {
      console.log(`question update failed (${updateRes.status}): ${updateRes.body}`);
      adminErrors.add(1);
    }

    sleep(0.2);

    const delRes = http.del(
      `${API_URL}/campaigns/${CAMPAIGN}/questions/${qId}`,
      null,
      { headers: h, tags: { name: "admin_delete_question" } }
    );
    const deleted = check(delRes, { "question deleted 200": (r) => r.status === 200 });
    if (!deleted) {
      console.log(`question delete failed (${delRes.status}): ${delRes.body}`);
      adminErrors.add(1);
    }
  });

  sleep(0.3);

  group("admin_prize_lifecycle", () => {
    const createRes = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/prizes`,
      JSON.stringify({ name: `Stress Prize ${uuidv4().slice(0, 4)}`, isRealPrize: false, order: 999 }),
      { headers: h, tags: { name: "admin_create_prize" } }
    );
    const created = check(createRes, {
      "prize created 201": (r) => r.status === 201,
      "prize create returned id": (r) => {
        try { return !!r.json("data.id"); } catch { return false; }
      },
    });
    if (!created) { adminErrors.add(1); return; }

    let pId;
    try { pId = createRes.json("data.id"); } catch { adminErrors.add(1); return; }
    if (!pId) { adminErrors.add(1); return; }

    sleep(0.2);

    const delRes = http.del(
      `${API_URL}/campaigns/${CAMPAIGN}/prizes/${pId}`,
      null,
      { headers: h, tags: { name: "admin_delete_prize" } }
    );
    const deleted = check(delRes, { "prize deleted 200": (r) => r.status === 200 });
    if (!deleted) {
      console.log(`prize delete failed (${delRes.status}): ${delRes.body}`);
      adminErrors.add(1);
    }
  });

  sleep(0.3);

  group("admin_settings_update", () => {
    const putRes = http.put(
      `${API_URL}/campaigns/${CAMPAIGN}/settings`,
      JSON.stringify({ staggerMode: "linear", geoEnforcement: "reject" }),
      { headers: h, tags: { name: "admin_put_settings" } }
    );
    check(putRes, { "settings update 200": (r) => r.status === 200 });
  });

  sleep(1);
}

// Legacy alias for soak profile admin reads
export function scenarioAdminCrud() {
  scenarioAdminReadOnly();
}

// ─── Scenario 4: Raffle System ────────────────────────────────────────────────

export function scenarioRaffle() {
  if (!ADMIN_TOKEN) { sleep(10); return; }

  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }

  const h = headers(csrf, ADMIN_TOKEN);

  group("raffle_list", () => {
    const res = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/raffles`,
      { headers: h, tags: { name: "raffle_list" } }
    );
    check(res, { "raffles 200": (r) => r.status === 200 });
  });

  sleep(0.5);

  // Create a raffle picking 1 winner, any score
  group("raffle_create", () => {
    const createRes = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/raffles`,
      JSON.stringify({ winnerCount: 1, minScore: 0, prizeWinnersOnly: false }),
      { headers: h, tags: { name: "raffle_create" } }
    );

    // 201 = raffle created; 422 = no submissions yet — both acceptable
    const ok = check(createRes, {
      "raffle created or empty": (r) => [201, 422].includes(r.status),
    });
    if (!ok) adminErrors.add(1);

    if (createRes.status !== 201) return;

    let raffleId, winners;
    try {
      raffleId = createRes.json("data.raffle.id");
      winners  = createRes.json("data.winners");
    } catch {
      adminErrors.add(1);
      return;
    }

    if (!raffleId) {
      console.log(`raffle create missing id: ${createRes.body}`);
      adminErrors.add(1);
      return;
    }

    sleep(0.3);

    const listRes = http.get(
      `${API_URL}/campaigns/${CAMPAIGN}/raffles/${raffleId}/winners`,
      { headers: h, tags: { name: "raffle_winners_list" } }
    );
    check(listRes, { "winners list 200": (r) => r.status === 200 });

    if (Array.isArray(winners) && winners.length > 0) {
      const winnerId = winners[0].id;
      if (winnerId) {
        const patchRes = http.patch(
          `${API_URL}/campaigns/${CAMPAIGN}/raffles/winners/${winnerId}`,
          JSON.stringify({ giftReceived: true }),
          { headers: h, tags: { name: "raffle_gift_toggle" } }
        );
        check(patchRes, { "gift toggled 200": (r) => r.status === 200 });
      }
    }
  });

  sleep(2);
}

// ─── Scenario 5: Rate-Limit Probing ──────────────────────────────────────────

export function scenarioRateLimitProbe() {
  const csrf = getCsrfToken();
  if (!csrf) { sleep(1); return; }

  // Hammer registration with the same IP using a shared sessionId pattern
  group("rate_limit_registration", () => {
    const sessionId = uuidv4();
    const { first, last } = randomPlayerName();

    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first,
        lastName:  last,
        sessionId,
        lat:       GEO_INSIDE.lat,
        lng:       GEO_INSIDE.lng,
        deviceId:  uuidv4(),
      }),
      { headers: headers(csrf), tags: { name: "rate_limit_reg" } }
    );

    // 201 = first success, 409 = dupe, 429 = rate limited — all expected
    const acceptable = check(res, {
      "reg rate probe acceptable": (r) => [201, 409, 429, 403].includes(r.status),
    });
    if (res.status === 429) rateLimitHits.add(1);
    if (!acceptable) registrationErrors.add(1);
  });

  sleep(0.1); // no sleep between attempts to trigger rate limit

  // Hammer spin with a fake/expired token to probe spin rate limiter
  group("rate_limit_spin", () => {
    const fakeToken = "invalid.token.stress";
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/spin`,
      JSON.stringify({ spinToken: fakeToken }),
      { headers: headers(csrf), tags: { name: "rate_limit_spin" } }
    );
    // 400 INVALID TOKEN or 429 RATE LIMITED — both show the limiter is active
    const ok = check(res, {
      "spin probe 400 or 429": (r) => [400, 429].includes(r.status),
    });
    if (res.status === 429) rateLimitHits.add(1);
  });

  sleep(0.5);
}

// ─── Scenario 6: Pagination / Cursor Stress ───────────────────────────────────

export function scenarioPaginationStress() {
  if (!ADMIN_TOKEN) { sleep(5); return; }

  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }
  const h = headers(csrf, ADMIN_TOKEN);

  // Walk through all submissions pages
  group("submissions_pagination", () => {
    let cursor = null;
    let page   = 0;
    let hasMore = true;

    while (hasMore && page < 10) {
      const url = cursor
        ? `${API_URL}/campaigns/${CAMPAIGN}/submissions?limit=20&cursor=${encodeURIComponent(cursor)}`
        : `${API_URL}/campaigns/${CAMPAIGN}/submissions?limit=20`;

      const res = http.get(url, { headers: h, tags: { name: "submissions_page" } });
      const ok = check(res, { "subs page 200": (r) => r.status === 200 });
      if (!ok) { adminErrors.add(1); break; }

      try {
        hasMore = res.json("hasMore");
        cursor  = res.json("nextCursor");
      } catch {
        break;
      }

      page++;
      sleep(0.1);
    }
  });

  sleep(0.5);

  // Walk through registrations pages
  group("registrations_pagination", () => {
    let cursor = null;
    let page   = 0;
    let hasMore = true;

    while (hasMore && page < 10) {
      const url = cursor
        ? `${API_URL}/campaigns/${CAMPAIGN}/registrations?limit=20&cursor=${encodeURIComponent(cursor)}`
        : `${API_URL}/campaigns/${CAMPAIGN}/registrations?limit=20`;

      const res = http.get(url, { headers: h, tags: { name: "registrations_page" } });
      const ok = check(res, { "regs page 200": (r) => r.status === 200 });
      if (!ok) { adminErrors.add(1); break; }

      try {
        hasMore = res.json("hasMore");
        cursor  = res.json("nextCursor");
      } catch {
        break;
      }

      page++;
      sleep(0.1);
    }
  });

  sleep(1);
}

// ─── Scenario 7: Duplicate Registration Flood ────────────────────────────────

export function scenarioDuplicateFlood() {
  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }

  const { first, last } = randomPlayerName();
  const deviceId        = uuidv4(); // same device for all attempts
  const sessionId       = uuidv4();

  // First registration — should succeed
  group("first_registration", () => {
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first,
        lastName:  last,
        sessionId,
        lat:       GEO_INSIDE.lat,
        lng:       GEO_INSIDE.lng,
        deviceId,
        deviceFingerprint: { deviceId, fingerprintHash: "deadbeef" },
      }),
      { headers: headers(csrf), tags: { name: "dup_first_reg" } }
    );
    check(res, { "first reg 201 or 409": (r) => [201, 409].includes(r.status) });
  });

  sleep(0.2);

  // Second attempt with the SAME device ID — must be blocked
  group("duplicate_same_device", () => {
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first + "2",
        lastName:  last,
        sessionId: uuidv4(), // different session
        lat:       GEO_INSIDE.lat,
        lng:       GEO_INSIDE.lng,
        deviceId,            // same device — should trigger DEVICE_ALREADY_USED
        deviceFingerprint: { deviceId },
      }),
      { headers: headers(csrf), tags: { name: "dup_same_device" } }
    );
    const blocked = check(res, {
      "duplicate device blocked": (r) => r.status === 409 || r.status === 429,
    });
    if (blocked) duplicateBlocks.add(1);
  });

  sleep(0.2);

  // Third attempt with the SAME normalized name — must be blocked
  group("duplicate_same_name", () => {
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first,
        lastName:  last,
        sessionId: uuidv4(),
        lat:       GEO_INSIDE.lat,
        lng:       GEO_INSIDE.lng,
        deviceId:  uuidv4(), // different device
      }),
      { headers: headers(csrf), tags: { name: "dup_same_name" } }
    );
    const blocked = check(res, {
      "duplicate name blocked": (r) => r.status === 409,
    });
    if (blocked) duplicateBlocks.add(1);
  });

  sleep(1);
}

// ─── Scenario 8: Spin Token Replay Attack ────────────────────────────────────

export function scenarioSpinReplay() {
  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }

  // Complete a full quiz to get a real spin token
  const { first, last } = randomPlayerName();
  const sessionId       = uuidv4();

  const regRes = http.post(
    `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
    JSON.stringify({
      firstName: first,
      lastName:  last,
      sessionId,
      lat:       GEO_INSIDE.lat,
      lng:       GEO_INSIDE.lng,
      deviceId:  uuidv4(),
    }),
    { headers: headers(csrf), tags: { name: "replay_register" } }
  );
  if (regRes.status !== 201) { sleep(2); return; }

  const questions = fetchQuestions(csrf);
  if (!questions || questions.length === 0) { sleep(2); return; }

  const subRes = http.post(
    `${API_URL}/campaigns/${CAMPAIGN}/submissions`,
    JSON.stringify({ sessionId, answers: buildAnswers(questions, true) }),
    { headers: headers(csrf), tags: { name: "replay_submit" } }
  );
  if (subRes.status !== 201) { sleep(2); return; }

  let spinToken;
  try { spinToken = subRes.json("data.spinToken"); } catch { sleep(2); return; }
  if (!spinToken) { sleep(2); return; }

  // First spin — should succeed
  group("replay_first_spin", () => {
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/spin`,
      JSON.stringify({ spinToken }),
      { headers: headers(csrf), tags: { name: "replay_spin_1" } }
    );
    check(res, { "first spin ok": (r) => r.status === 200 });
  });

  sleep(0.3);

  // Second spin with the SAME token — must be rejected or return idempotent result
  group("replay_second_spin", () => {
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/spin`,
      JSON.stringify({ spinToken }),
      { headers: headers(csrf), tags: { name: "replay_spin_2" } }
    );
    const ok = check(res, {
      "replay blocked or idempotent": (r) => {
        if (r.status === 409) return true; // SPIN_TOKEN_ALREADY_USED
        if (r.status === 200) {
          try { return r.json("data.idempotent") === true; } catch { return false; }
        }
        return false;
      },
    });
    if (!ok) spinErrors.add(1);
  });

  sleep(1);
}

// ─── Scenario 9: Geofence Boundary Cases ─────────────────────────────────────

export function scenarioGeoBoundary() {
  const csrf = getCsrfToken();
  if (!csrf) { sleep(2); return; }

  // Case A: clearly inside zone
  group("geo_inside_zone", () => {
    const { first, last } = randomPlayerName();
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first,
        lastName:  last,
        sessionId: uuidv4(),
        lat:       GEO_INSIDE.lat,
        lng:       GEO_INSIDE.lng,
        deviceId:  uuidv4(),
      }),
      { headers: headers(csrf), tags: { name: "geo_inside" } }
    );
    check(res, { "inside geo: 201 or 409": (r) => [201, 409].includes(r.status) });
  });

  sleep(0.3);

  // Case B: clearly outside zone — should be rejected with GEO_OUTSIDE_ZONE
  group("geo_outside_zone", () => {
    const { first, last } = randomPlayerName();
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first,
        lastName:  last,
        sessionId: uuidv4(),
        lat:       GEO_OUTSIDE.lat,
        lng:       GEO_OUTSIDE.lng,
        deviceId:  uuidv4(),
      }),
      { headers: headers(csrf), tags: { name: "geo_outside" } }
    );
    const blocked = check(res, {
      "outside geo blocked 403": (r) => {
        // 403 GEO_OUTSIDE_ZONE = zones configured and enforced
        // 201 = no zones configured (no_zones mode), both are acceptable
        return [403, 201, 409].includes(r.status);
      },
    });
    if (res.status === 403) geoBlocks.add(1);
  });

  sleep(0.3);

  // Case C: exactly at zone boundary (edge lat/lng, within jitter)
  group("geo_boundary_edge", () => {
    const { first, last } = randomPlayerName();
    // 499 m from Nairobi CBD centre — within a 500 m zone, outside a 100 m zone
    const edgeLat = GEO_INSIDE.lat + 0.0045; // ~500 m north
    const edgeLng = GEO_INSIDE.lng;

    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: first,
        lastName:  last,
        sessionId: uuidv4(),
        lat:       edgeLat,
        lng:       edgeLng,
        deviceId:  uuidv4(),
      }),
      { headers: headers(csrf), tags: { name: "geo_edge" } }
    );
    check(res, { "boundary: any valid http code": (r) => r.status < 500 });
  });

  sleep(0.3);

  // Case D: invalid coordinates (lat > 90)
  group("geo_invalid_coords", () => {
    const res = http.post(
      `${API_URL}/campaigns/${CAMPAIGN}/registrations`,
      JSON.stringify({
        firstName: "INVALID",
        lastName:  "COORDS",
        sessionId: uuidv4(),
        lat:       999,
        lng:       999,
        deviceId:  uuidv4(),
      }),
      { headers: headers(csrf), tags: { name: "geo_invalid" } }
    );
    check(res, { "invalid coords rejected 400": (r) => r.status === 400 });
  });

  sleep(1);
}

// ─── Scenario 10: Soak Test ───────────────────────────────────────────────────

export function scenarioSoak() {
  // Lightweight mixed workload simulating real event background traffic
  const actions = [
    () => { // Public: read questions
      const csrf = getCsrfToken();
      if (!csrf) return;
      http.get(
        `${API_URL}/campaigns/${CAMPAIGN}/questions`,
        { headers: headers(csrf), tags: { name: "soak_questions" } }
      );
    },
    () => { // Public: read settings
      const csrf = getCsrfToken();
      if (!csrf) return;
      http.get(
        `${API_URL}/campaigns/${CAMPAIGN}/settings`,
        { headers: headers(csrf), tags: { name: "soak_settings" } }
      );
    },
    () => { // Admin: list submissions (if token available)
      if (!ADMIN_TOKEN) return;
      const csrf = getCsrfToken();
      if (!csrf) return;
      http.get(
        `${API_URL}/campaigns/${CAMPAIGN}/submissions?limit=10`,
        { headers: headers(csrf, ADMIN_TOKEN), tags: { name: "soak_admin_subs" } }
      );
    },
    () => { // Public: campaign prizes
      const csrf = getCsrfToken();
      if (!csrf) return;
      http.get(
        `${API_URL}/campaigns/${CAMPAIGN}/prizes`,
        { headers: headers(csrf), tags: { name: "soak_prizes" } }
      );
    },
    () => { // Health check
      http.get(`${API_URL.replace("/api", "")}/health`, { tags: { name: "soak_health" } });
    },
  ];

  const action = actions[Math.floor(Math.random() * actions.length)];
  action();

  sleep(Math.random() * 2 + 0.5); // 0.5–2.5 s between requests
}

// ─── Setup / Teardown ─────────────────────────────────────────────────────────

export function setup() {
  console.log(`\n${"=".repeat(60)}`);
  console.log(`  PowerUpGameOn Stress Test`);
  console.log(`  API:      ${API_URL}`);
  console.log(`  Campaign: ${CAMPAIGN}`);
  console.log(`  Admin:    ${ADMIN_TOKEN ? "provided ✓" : "not provided (admin scenarios skipped)"}`);
  console.log(`${"=".repeat(60)}\n`);

  // Validate the API is reachable before starting
  const health = http.get(`${API_URL.replace("/api", "")}/health`);
  if (health.status !== 200) {
    fail(`API health check failed (${health.status}) — is the server running at ${API_URL}?`);
  }

  // Validate campaign exists
  const csrf = getCsrfToken();
  if (!csrf) fail("Could not obtain CSRF token — check API_CSRF_SECRET configuration.");

  const settingsRes = http.get(
    `${API_URL}/campaigns/${CAMPAIGN}/settings`,
    { headers: { "X-CSRF-Token": csrf } }
  );
  if (settingsRes.status === 404) {
    fail(`Campaign "${CAMPAIGN}" not found. Create it first or set CAMPAIGN_SLUG env var.`);
  }

  console.log("Setup complete — all pre-checks passed.\n");
  return { startedAt: new Date().toISOString() };
}

export function teardown(data) {
  console.log(`\n${"=".repeat(60)}`);
  console.log(`  Test completed`);
  console.log(`  Started:    ${data.startedAt}`);
  console.log(`  Finished:   ${new Date().toISOString()}`);
  console.log(`\n  Check the summary above for threshold results.`);
  console.log(`${"=".repeat(60)}\n`);
}
