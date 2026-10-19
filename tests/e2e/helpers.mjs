/**
 * @file helpers.mjs
 * @description Shared helpers for the Perigee API e2e test suite.
 *
 * Loads config from environment variables (optionally from tests/e2e/.env).
 * All helpers return plain objects / throw on non-OK responses.
 *
 * API-40: End-to-end coverage of provision → report → reconcile.
 */

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Try to load a local .env file (tests/e2e/.env).  Any values already set in
 * the process environment take precedence — this mirrors how dotenv works.
 */
function loadDotEnv() {
  const envPath = resolve(__dirname, ".env");
  try {
    const raw = readFileSync(envPath, "utf8");
    for (const line of raw.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      const eqIdx = trimmed.indexOf("=");
      if (eqIdx < 1) continue;
      const key = trimmed.slice(0, eqIdx).trim();
      const value = trimmed.slice(eqIdx + 1).trim();
      if (!(key in process.env)) {
        process.env[key] = value;
      }
    }
  } catch {
    // .env is optional — the suite also runs via env vars injected by CI.
  }
}

loadDotEnv();

export const config = {
  apiBaseUrl: (process.env.API_BASE_URL || "http://localhost:8080").replace(/\/+$/, ""),
  sorobanRpcUrl: process.env.SOROBAN_RPC_URL || "https://soroban-testnet.stellar.org",
  managerStellarAddress: process.env.MANAGER_STELLAR_ADDRESS || "",
  reconcileTimeoutMs: parseInt(process.env.RECONCILE_TIMEOUT_MS || "30000", 10),
  reconcileTolerancePct: parseFloat(process.env.RECONCILE_TOLERANCE_PCT || "10"),
};

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

/**
 * Make a JSON request to the Perigee API.
 *
 * @param {string} method   HTTP method
 * @param {string} path     Path relative to apiBaseUrl (must start with /)
 * @param {object} [body]   Request body (serialised to JSON)
 * @param {Record<string,string>} [headers]  Extra headers
 * @returns {Promise<{status: number, body: unknown}>}
 */
export async function apiRequest(method, path, body = undefined, headers = {}) {
  const url = `${config.apiBaseUrl}${path}`;
  const init = {
    method,
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
      ...headers,
    },
  };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }

  const res = await fetch(url, init);
  let responseBody;
  const contentType = res.headers.get("content-type") || "";
  if (contentType.includes("application/json")) {
    responseBody = await res.json();
  } else {
    responseBody = await res.text();
  }

  return { status: res.status, body: responseBody };
}

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

/**
 * Obtain a JWT by going through the challenge/verify flow.
 *
 * The Perigee auth flow is:
 *   1. POST /auth/challenge  → { challenge }
 *   2. POST /auth/verify     → { token }
 *
 * In test mode (no real signing key) the server accepts an unsigned
 * challenge response for any Stellar address.  This helper mirrors what
 * a real client would do without requiring a live wallet.
 *
 * @param {string} stellarAddress  Stellar public key (G...)
 * @returns {Promise<string>}      JWT bearer token
 */
export async function obtainAuthToken(stellarAddress) {
  // Step 1 — request a challenge
  const challengeRes = await apiRequest("POST", "/auth/challenge", {
    stellar_address: stellarAddress,
  });

  if (challengeRes.status !== 200 && challengeRes.status !== 201) {
    throw new Error(
      `Challenge request failed with status ${challengeRes.status}: ${JSON.stringify(challengeRes.body)}`
    );
  }

  const challenge =
    challengeRes.body?.challenge ??
    challengeRes.body?.data?.challenge ??
    challengeRes.body;

  // Step 2 — verify (the dev server accepts unsigned responses)
  const verifyRes = await apiRequest("POST", "/auth/verify", {
    stellar_address: stellarAddress,
    challenge,
    signature: "00".repeat(64), // 64-byte zero signature — accepted in dev/test mode
  });

  if (verifyRes.status !== 200 && verifyRes.status !== 201) {
    throw new Error(
      `Verify request failed with status ${verifyRes.status}: ${JSON.stringify(verifyRes.body)}`
    );
  }

  const token =
    verifyRes.body?.token ??
    verifyRes.body?.access_token ??
    verifyRes.body?.data?.token;

  if (!token) {
    throw new Error(`No token in verify response: ${JSON.stringify(verifyRes.body)}`);
  }

  return token;
}

// ---------------------------------------------------------------------------
// Vault helpers
// ---------------------------------------------------------------------------

/**
 * Provision a new vault via POST /vaults.
 *
 * @param {string} token         JWT bearer token
 * @param {object} overrides     Optional field overrides for CreateVaultRequest
 * @returns {Promise<{status: number, body: object}>}
 */
export async function provisionVault(token, overrides = {}) {
  const payload = {
    manager_id: config.managerStellarAddress || "test-manager-id",
    name: `e2e-vault-${Date.now()}`,
    status: "active",
    config_json: JSON.stringify({
      strategy: "btc_eth_basket",
      testnet: true,
    }),
    idempotency_key: `e2e-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    ...overrides,
  };

  return apiRequest("POST", "/vaults", payload, {
    Authorization: `Bearer ${token}`,
  });
}

// ---------------------------------------------------------------------------
// Fee analytics helpers (report step)
// ---------------------------------------------------------------------------

/**
 * Fetch the fee analytics report from GET /fees/analytics.
 *
 * @returns {Promise<{status: number, body: object}>}
 */
export async function fetchFeeAnalytics() {
  return apiRequest("GET", "/fees/analytics");
}

// ---------------------------------------------------------------------------
// Reconciliation helpers
// ---------------------------------------------------------------------------

/**
 * Trigger a reconcile job via POST /reconcile and return the job details.
 *
 * @param {object} params  { from_ledger, to_ledger, tolerance_pct }
 * @returns {Promise<{status: number, body: object}>}
 */
export async function startReconcile(params) {
  const { from_ledger, to_ledger, tolerance_pct = config.reconcileTolerancePct } = params;
  return apiRequest("POST", "/reconcile", { from_ledger, to_ledger, tolerance_pct });
}

/**
 * Poll GET /reconcile/:job_id until the job reaches a terminal state
 * ("completed" or "failed") or the timeout expires.
 *
 * @param {string} jobId
 * @param {number} [timeoutMs]
 * @returns {Promise<object>}  Final job record
 */
export async function waitForReconcileJob(jobId, timeoutMs = config.reconcileTimeoutMs) {
  const deadline = Date.now() + timeoutMs;
  const intervalMs = 1000;

  while (Date.now() < deadline) {
    const res = await apiRequest("GET", `/reconcile/${jobId}`);
    if (res.status === 200) {
      const status = res.body?.status;
      if (status === "completed" || status === "failed") {
        return res.body;
      }
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }

  throw new Error(`Reconcile job ${jobId} did not complete within ${timeoutMs}ms`);
}

/**
 * Fetch reconciliation reports via GET /reconcile/reports.
 *
 * @param {object} [query]  Query params, e.g. { limit: 5 }
 * @returns {Promise<{status: number, body: unknown}>}
 */
export async function listReconcileReports(query = {}) {
  const qs = new URLSearchParams(
    Object.fromEntries(
      Object.entries(query).map(([k, v]) => [k, String(v)])
    )
  ).toString();
  const path = qs ? `/reconcile/reports?${qs}` : "/reconcile/reports";
  return apiRequest("GET", path);
}

// ---------------------------------------------------------------------------
// Stellar testnet helpers
// ---------------------------------------------------------------------------

/**
 * Fetch the latest ledger sequence from the Soroban testnet RPC.
 * Used to build valid `from_ledger` / `to_ledger` values for reconciliation.
 *
 * @returns {Promise<number>}  Latest ledger sequence number
 */
export async function getLatestTestnetLedger() {
  const res = await fetch(config.sorobanRpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "getLatestLedger",
      params: {},
    }),
  });

  if (!res.ok) {
    throw new Error(`Stellar RPC getLatestLedger failed: ${res.status}`);
  }

  const data = await res.json();
  const seq = data?.result?.sequence;
  if (!seq) {
    throw new Error(`Unexpected Stellar RPC response: ${JSON.stringify(data)}`);
  }
  return Number(seq);
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

/**
 * Lightweight assert that throws a descriptive error on failure.
 *
 * @param {boolean} condition
 * @param {string} message
 */
export function assert(condition, message) {
  if (!condition) {
    throw new Error(`Assertion failed: ${message}`);
  }
}

/**
 * Assert that `value` is an integer and optionally satisfies a range.
 *
 * @param {unknown} value
 * @param {string} label
 * @param {{ min?: number, max?: number }} [range]
 */
export function assertInteger(value, label, range = {}) {
  assert(Number.isInteger(Number(value)), `${label} should be an integer, got ${JSON.stringify(value)}`);
  if (range.min !== undefined) {
    assert(Number(value) >= range.min, `${label} should be >= ${range.min}, got ${value}`);
  }
  if (range.max !== undefined) {
    assert(Number(value) <= range.max, `${label} should be <= ${range.max}, got ${value}`);
  }
}

/**
 * Assert that `obj` has all the listed keys.
 *
 * @param {object} obj
 * @param {string[]} keys
 * @param {string} label
 */
export function assertHasKeys(obj, keys, label) {
  assert(obj !== null && typeof obj === "object", `${label} should be an object`);
  for (const key of keys) {
    assert(key in obj, `${label} is missing required field '${key}' — got: ${JSON.stringify(Object.keys(obj))}`);
  }
}
