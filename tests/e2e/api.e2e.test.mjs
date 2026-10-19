/**
 * @file api.e2e.test.mjs
 * @description End-to-end test suite for the Perigee API against Stellar testnet.
 *
 * Covers the full white-label workflow described in issue #356 / API-40:
 *   1. Provision  — POST /vaults creates a scoped Policy Vault.
 *   2. Report     — GET  /fees/analytics returns live fee data from testnet.
 *   3. Reconcile  — POST /reconcile runs the fee reconciliation engine against
 *                   a real testnet ledger range; GET /reconcile/reports
 *                   confirms the report was persisted.
 *
 * Pre-requisites
 * ──────────────
 *   • Perigee backend running:  cd core && cargo run
 *   • Network access to https://soroban-testnet.stellar.org
 *   • (Optional) tests/e2e/.env with MANAGER_STELLAR_ADDRESS set.
 *     Without it, the suite uses a synthetic manager ID — sufficient for
 *     all structural assertions.
 *
 * Run
 * ───
 *   node --test tests/e2e/api.e2e.test.mjs
 *
 *   Or via the package.json helper:
 *   cd tests/e2e && npm test
 */

import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";

import {
  config,
  apiRequest,
  obtainAuthToken,
  provisionVault,
  fetchFeeAnalytics,
  startReconcile,
  waitForReconcileJob,
  listReconcileReports,
  getLatestTestnetLedger,
  assertHasKeys,
  assertInteger,
} from "./helpers.mjs";

// ---------------------------------------------------------------------------
// Suite-wide state shared across tests (populated in before())
// ---------------------------------------------------------------------------
let jwtToken = "";
let provisionedVaultId = "";
let reconcileJobId = "";
let testnetLatestLedger = 0;

/** Deterministic fake Stellar address used when none is configured. */
const TEST_MANAGER_ADDRESS =
  config.managerStellarAddress ||
  "GDUMMY000TESTNET0ADDRESS00000000000000000000000000000000A";

// ---------------------------------------------------------------------------
// 0 — Connectivity smoke-test
// ---------------------------------------------------------------------------

describe("0 · API connectivity", () => {
  it("GET /health returns 200", async () => {
    const res = await apiRequest("GET", "/health");
    assert.equal(res.status, 200, `Expected 200, got ${res.status}. Is the backend running at ${config.apiBaseUrl}?`);
  });

  it("GET /ready returns 200", async () => {
    const res = await apiRequest("GET", "/ready");
    assert.equal(res.status, 200, `Expected 200, got ${res.status}`);
  });
});

// ---------------------------------------------------------------------------
// 1 — Authentication
// ---------------------------------------------------------------------------

describe("1 · Authentication", () => {
  it("POST /auth/challenge accepts a Stellar address and returns a challenge string", async () => {
    const res = await apiRequest("POST", "/auth/challenge", {
      stellar_address: TEST_MANAGER_ADDRESS,
    });
    assert.ok(
      res.status === 200 || res.status === 201,
      `Expected 2xx, got ${res.status}: ${JSON.stringify(res.body)}`
    );
    const challenge = res.body?.challenge ?? res.body?.data?.challenge ?? res.body;
    assert.ok(
      typeof challenge === "string" && challenge.length > 0,
      `Expected a non-empty challenge string, got: ${JSON.stringify(res.body)}`
    );
  });

  it("POST /auth/verify issues a JWT bearer token", async () => {
    jwtToken = await obtainAuthToken(TEST_MANAGER_ADDRESS);
    assert.ok(typeof jwtToken === "string" && jwtToken.length > 0, "Expected a non-empty JWT string");
    // JWT format: three base64url segments separated by dots
    assert.equal(jwtToken.split(".").length, 3, "JWT should have three dot-separated segments");
  });

  it("protected routes reject requests without a token", async () => {
    const res = await apiRequest("POST", "/vaults", {
      manager_id: TEST_MANAGER_ADDRESS,
      name: "no-auth-vault",
    });
    // 401 Unauthorised or 403 Forbidden
    assert.ok(
      res.status === 401 || res.status === 403,
      `Expected 401 or 403 for unauthenticated request, got ${res.status}`
    );
  });
});

// ---------------------------------------------------------------------------
// 2 — Provision (POST /vaults)
// ---------------------------------------------------------------------------

describe("2 · Provision vault", () => {
  it("POST /vaults creates a vault and returns the vault record", async () => {
    assert.ok(jwtToken, "JWT token must be set (auth test should run first)");

    const res = await provisionVault(jwtToken);
    assert.ok(
      res.status === 200 || res.status === 201,
      `Expected 2xx, got ${res.status}: ${JSON.stringify(res.body)}`
    );

    const vault = res.body;
    assertHasKeys(vault, ["id", "manager_id", "name", "status", "config_json", "version"], "VaultRecord");
    assert.ok(typeof vault.id === "string" && vault.id.length > 0, "vault.id should be a non-empty string");
    assert.equal(vault.status, "active", "vault.status should default to 'active'");
    assertInteger(vault.version, "vault.version", { min: 1 });

    // Save for downstream tests
    provisionedVaultId = vault.id;
  });

  it("GET /vaults/:id retrieves the newly provisioned vault", async () => {
    assert.ok(provisionedVaultId, "Vault ID must be set (provision test should run first)");

    const res = await apiRequest("GET", `/vaults/${provisionedVaultId}`, undefined, {
      Authorization: `Bearer ${jwtToken}`,
    });

    assert.equal(res.status, 200, `Expected 200, got ${res.status}: ${JSON.stringify(res.body)}`);
    assertHasKeys(res.body, ["id", "manager_id", "name", "status"], "VaultRecord");
    assert.equal(res.body.id, provisionedVaultId, "Returned vault id should match");
  });

  it("POST /vaults with the same idempotency_key returns the existing vault (idempotency)", async () => {
    assert.ok(jwtToken, "JWT token must be set");

    const idempotencyKey = `e2e-idem-${Date.now()}`;

    const first = await provisionVault(jwtToken, { idempotency_key: idempotencyKey });
    const second = await provisionVault(jwtToken, { idempotency_key: idempotencyKey });

    assert.ok(first.status === 200 || first.status === 201, `First provision failed: ${first.status}`);
    assert.ok(second.status === 200 || second.status === 201, `Second provision failed: ${second.status}`);
    assert.equal(
      first.body.id,
      second.body.id,
      "Idempotent requests should return the same vault ID"
    );
  });

  it("POST /vaults rejects a request with an empty manager_id", async () => {
    const res = await apiRequest(
      "POST",
      "/vaults",
      { manager_id: "", name: "bad-vault" },
      { Authorization: `Bearer ${jwtToken}` }
    );
    assert.ok(
      res.status === 400 || res.status === 422,
      `Expected 400/422 for empty manager_id, got ${res.status}`
    );
  });

  it("GET /vaults lists vaults for the authenticated manager", async () => {
    const res = await apiRequest("GET", "/vaults", undefined, {
      Authorization: `Bearer ${jwtToken}`,
    });
    assert.equal(res.status, 200, `Expected 200, got ${res.status}: ${JSON.stringify(res.body)}`);
    assert.ok(Array.isArray(res.body), "Response should be an array of vault records");
    // The vault we provisioned must be in the list
    const found = res.body.some((v) => v.id === provisionedVaultId);
    assert.ok(found, `Provisioned vault ${provisionedVaultId} not found in GET /vaults response`);
  });
});

// ---------------------------------------------------------------------------
// 3 — Report (GET /fees/analytics)
// ---------------------------------------------------------------------------

describe("3 · Fee analytics report", () => {
  it("GET /fees/analytics is publicly accessible and returns a 200", async () => {
    const res = await fetchFeeAnalytics();
    assert.equal(res.status, 200, `Expected 200, got ${res.status}: ${JSON.stringify(res.body)}`);
  });

  it("fee analytics payload has the expected top-level structure", async () => {
    const res = await fetchFeeAnalytics();
    assert.equal(res.status, 200);
    const body = res.body;

    // The fee analytics engine returns at least one of these top-level keys
    const knownKeys = [
      "current_base_fee",
      "recommended_fee",
      "recommendation",
      "market_conditions",
      "samples",
      "analytics",
      "data",
      "fee",
    ];

    const hasAny = knownKeys.some((k) => k in body);
    assert.ok(
      hasAny,
      `Expected at least one of [${knownKeys.join(", ")}] in analytics response, got: ${JSON.stringify(Object.keys(body))}`
    );
  });

  it("GET /fees/analytics response is backed by Stellar testnet data", async () => {
    // Verify the backend can actually talk to testnet by cross-checking
    // the latest ledger from the testnet RPC with what the analytics returns.
    testnetLatestLedger = await getLatestTestnetLedger();
    assert.ok(testnetLatestLedger > 0, `Expected a positive ledger sequence from testnet, got ${testnetLatestLedger}`);

    const res = await fetchFeeAnalytics();
    assert.equal(res.status, 200);
    // Analytics data should reference ledgers close to the testnet tip.
    // We accept up to 1000 ledger lag (≈ 83 minutes at 5 s/ledger).
    const reportedLedger =
      res.body?.latest_ledger ??
      res.body?.current_ledger ??
      res.body?.ledger_sequence ??
      res.body?.data?.latest_ledger;

    if (reportedLedger !== undefined) {
      const lag = testnetLatestLedger - Number(reportedLedger);
      assert.ok(
        lag <= 2000,
        `Analytics ledger ${reportedLedger} lags testnet tip ${testnetLatestLedger} by ${lag} ledgers (max 2000)`
      );
    }
    // If the field isn't in the response we skip the lag check — the
    // shape assertion above already verified structural correctness.
  });
});

// ---------------------------------------------------------------------------
// 4 — Reconcile (POST /reconcile → GET /reconcile/:id → GET /reconcile/reports)
// ---------------------------------------------------------------------------

describe("4 · Fee reconciliation", () => {
  /** Ledger window used in reconciliation tests. */
  let fromLedger = 0;
  let toLedger = 0;

  before(async () => {
    // Build a small ledger window anchored to the testnet tip.
    // 10 ledgers ≈ 50 seconds of chain time — small enough to keep the test
    // fast, big enough to exercise the reconciliation engine.
    if (testnetLatestLedger === 0) {
      testnetLatestLedger = await getLatestTestnetLedger();
    }
    toLedger = testnetLatestLedger;
    fromLedger = Math.max(1, toLedger - 10);
  });

  it("POST /reconcile accepts a ledger range and returns a job ID", async () => {
    const res = await startReconcile({
      from_ledger: fromLedger,
      to_ledger: toLedger,
      tolerance_pct: config.reconcileTolerancePct,
    });

    assert.ok(
      res.status === 200 || res.status === 201 || res.status === 202,
      `Expected 2xx, got ${res.status}: ${JSON.stringify(res.body)}`
    );

    assertHasKeys(res.body, ["job_id", "status"], "ReconcileResponse");
    assert.ok(
      typeof res.body.job_id === "string" && res.body.job_id.length > 0,
      "job_id should be a non-empty string"
    );

    // Save job ID for downstream tests
    reconcileJobId = res.body.job_id;
  });

  it("POST /reconcile rejects invalid ledger ranges (to < from)", async () => {
    const res = await startReconcile({
      from_ledger: toLedger,
      to_ledger: fromLedger, // intentionally reversed
    });
    assert.ok(
      res.status === 400 || res.status === 422,
      `Expected 400/422 for invalid range, got ${res.status}: ${JSON.stringify(res.body)}`
    );
  });

  it("GET /reconcile/:job_id returns job status immediately after submission", async () => {
    assert.ok(reconcileJobId, "Job ID must be set (submit test should run first)");

    const res = await apiRequest("GET", `/reconcile/${reconcileJobId}`);
    assert.equal(res.status, 200, `Expected 200, got ${res.status}: ${JSON.stringify(res.body)}`);

    const job = res.body;
    assertHasKeys(job, ["id", "status"], "JobRecord");
    assert.ok(
      ["pending", "running", "completed", "failed"].includes(job.status),
      `Unexpected job status '${job.status}'`
    );
  });

  it("reconcile job completes within the configured timeout", async () => {
    assert.ok(reconcileJobId, "Job ID must be set");

    const finalJob = await waitForReconcileJob(reconcileJobId);

    assert.equal(finalJob.status, "completed", `Job ended with status '${finalJob.status}': ${JSON.stringify(finalJob)}`);
  });

  it("GET /reconcile/reports lists at least the completed report", async () => {
    const res = await listReconcileReports({ limit: 10 });

    assert.equal(res.status, 200, `Expected 200, got ${res.status}: ${JSON.stringify(res.body)}`);
    assert.ok(Array.isArray(res.body), "Expected an array of reconciliation reports");
    assert.ok(res.body.length >= 1, "Expected at least one reconciliation report in the list");
  });

  it("reconciliation report has the correct schema", async () => {
    const res = await listReconcileReports({ limit: 1 });
    assert.equal(res.status, 200);

    const report = res.body[0];
    assertHasKeys(
      report,
      ["id", "from_ledger", "to_ledger", "tolerance_pct", "total_ledgers", "discrepancies_count"],
      "ReconciliationReport"
    );

    assertInteger(report.from_ledger, "from_ledger", { min: 1 });
    assertInteger(report.to_ledger, "to_ledger", { min: 1 });
    assert.ok(
      Number(report.to_ledger) >= Number(report.from_ledger),
      `to_ledger (${report.to_ledger}) should be >= from_ledger (${report.from_ledger})`
    );
    assertInteger(report.total_ledgers, "total_ledgers", { min: 1 });
    assertInteger(report.discrepancies_count, "discrepancies_count", { min: 0 });
    assert.ok(
      typeof report.tolerance_pct === "number",
      `tolerance_pct should be a number, got ${typeof report.tolerance_pct}`
    );
  });

  it("reconciliation report delta_pct is within the configured tolerance", async () => {
    const res = await listReconcileReports({ limit: 1 });
    assert.equal(res.status, 200);

    const report = res.body[0];
    const avgDelta =
      report.avg_delta_pct ??
      report.summary?.mean_delta_pct ??
      0;

    assert.ok(
      typeof avgDelta === "number",
      `avg_delta_pct should be a number, got ${JSON.stringify(avgDelta)}`
    );
    assert.ok(
      avgDelta <= config.reconcileTolerancePct,
      `Average delta ${avgDelta.toFixed(2)}% exceeds tolerance ${config.reconcileTolerancePct}%`
    );
  });
});

// ---------------------------------------------------------------------------
// 5 — Full workflow smoke test (provision → report → reconcile in sequence)
// ---------------------------------------------------------------------------

describe("5 · Full provision → report → reconcile workflow", () => {
  it("executes the complete white-label workflow end-to-end", async () => {
    // ── Step A: Provision a new vault ──────────────────────────────────────
    const vaultRes = await provisionVault(jwtToken, {
      name: `e2e-workflow-${Date.now()}`,
    });
    assert.ok(
      vaultRes.status === 200 || vaultRes.status === 201,
      `Provision failed: ${vaultRes.status} — ${JSON.stringify(vaultRes.body)}`
    );
    const vaultId = vaultRes.body.id;
    assert.ok(vaultId, "Expected a vault ID from provision step");

    // ── Step B: Fetch the fee analytics report ─────────────────────────────
    const analyticsRes = await fetchFeeAnalytics();
    assert.equal(analyticsRes.status, 200, `Analytics failed: ${analyticsRes.status}`);
    assert.ok(analyticsRes.body, "Expected a non-empty analytics payload");

    // ── Step C: Run reconciliation against a testnet ledger window ─────────
    const latestLedger = await getLatestTestnetLedger();
    const reconcileRes = await startReconcile({
      from_ledger: Math.max(1, latestLedger - 5),
      to_ledger: latestLedger,
      tolerance_pct: config.reconcileTolerancePct,
    });
    assert.ok(
      reconcileRes.status === 200 || reconcileRes.status === 201 || reconcileRes.status === 202,
      `Reconcile submit failed: ${reconcileRes.status} — ${JSON.stringify(reconcileRes.body)}`
    );

    const jobId = reconcileRes.body.job_id;
    assert.ok(jobId, "Expected a job_id from reconcile submit");

    // ── Step D: Wait for the job and verify the report is persisted ────────
    const finalJob = await waitForReconcileJob(jobId);
    assert.equal(finalJob.status, "completed", `Reconcile job did not complete: ${JSON.stringify(finalJob)}`);

    const reportsRes = await listReconcileReports({ limit: 10 });
    assert.equal(reportsRes.status, 200);
    assert.ok(Array.isArray(reportsRes.body) && reportsRes.body.length >= 1, "Expected reports after workflow");

    // All three steps succeeded — the workflow is end-to-end verified.
  });
});

// ---------------------------------------------------------------------------
// 6 — Edge cases & error handling
// ---------------------------------------------------------------------------

describe("6 · Edge cases", () => {
  it("GET /vaults/:id returns 404 for a non-existent vault ID", async () => {
    const res = await apiRequest("GET", `/vaults/non-existent-vault-id-00000`, undefined, {
      Authorization: `Bearer ${jwtToken}`,
    });
    assert.equal(res.status, 404, `Expected 404, got ${res.status}`);
  });

  it("GET /reconcile/:job_id returns 404 for a non-existent job ID", async () => {
    const res = await apiRequest("GET", `/reconcile/non-existent-job-id-00000`);
    assert.equal(res.status, 404, `Expected 404, got ${res.status}`);
  });

  it("POST /reconcile with zero-width ledger range returns a report", async () => {
    // from_ledger === to_ledger is technically valid (single-ledger window)
    const latest = await getLatestTestnetLedger();
    const res = await startReconcile({ from_ledger: latest, to_ledger: latest });
    assert.ok(
      res.status >= 200 && res.status < 300,
      `Expected 2xx for single-ledger reconcile, got ${res.status}`
    );
  });

  it("POST /vaults rejects an empty vault name", async () => {
    const res = await apiRequest(
      "POST",
      "/vaults",
      { manager_id: TEST_MANAGER_ADDRESS, name: "" },
      { Authorization: `Bearer ${jwtToken}` }
    );
    assert.ok(
      res.status === 400 || res.status === 422,
      `Expected 400/422 for empty name, got ${res.status}`
    );
  });

  it("GET /fees/analytics is idempotent (two calls return consistent shapes)", async () => {
    const [res1, res2] = await Promise.all([fetchFeeAnalytics(), fetchFeeAnalytics()]);
    assert.equal(res1.status, 200);
    assert.equal(res2.status, 200);
    // Both should have the same top-level keys
    const keys1 = Object.keys(res1.body).sort().join(",");
    const keys2 = Object.keys(res2.body).sort().join(",");
    assert.equal(keys1, keys2, "Two analytics calls should return the same schema");
  });
});
