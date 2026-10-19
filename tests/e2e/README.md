# Perigee API — End-to-End Test Suite

> **Issue #356 / API-40** — _No end-to-end test against testnet._

This directory contains an end-to-end test suite that exercises the Perigee API against a running backend connected to Stellar testnet.  It covers the full white-label workflow:

```
Provision (POST /vaults)
    ↓
Report   (GET  /fees/analytics)
    ↓
Reconcile (POST /reconcile → poll GET /reconcile/:id → GET /reconcile/reports)
```

---

## Prerequisites

| Requirement | Notes |
|---|---|
| Node.js ≥ 18 | Uses the built-in `node:test` runner — no extra packages needed |
| Running Perigee backend | `cd core && cargo run` |
| Network access | Tests call `https://soroban-testnet.stellar.org` for ledger data |

---

## Quick start

```bash
# 1. Copy and configure the environment file
cp tests/e2e/.env.example tests/e2e/.env
# Edit tests/e2e/.env — at minimum set MANAGER_STELLAR_ADDRESS

# 2. Start the backend
cd core && cargo run &

# 3. Run the suite
node --test tests/e2e/api.e2e.test.mjs

# Or from the tests/e2e directory:
cd tests/e2e && npm test
```

---

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `API_BASE_URL` | `http://localhost:8080` | Base URL of the running Perigee API |
| `SOROBAN_RPC_URL` | `https://soroban-testnet.stellar.org` | Stellar testnet RPC URL |
| `MANAGER_STELLAR_ADDRESS` | _(synthetic)_ | Stellar address used to provision vaults. Fund it at the [Stellar Laboratory](https://laboratory.stellar.org/#account-creator?network=test) |
| `RECONCILE_TIMEOUT_MS` | `30000` | Max ms to wait for a reconcile job to complete |
| `RECONCILE_TOLERANCE_PCT` | `10` | Max acceptable average fee delta (%) |

---

## Test suites

| Suite | Description |
|---|---|
| `0 · API connectivity` | Health and readiness probes |
| `1 · Authentication` | Challenge/verify flow, JWT shape, auth guard |
| `2 · Provision vault` | POST /vaults, GET /vaults/:id, idempotency, validation |
| `3 · Fee analytics report` | GET /fees/analytics — structure and testnet freshness |
| `4 · Fee reconciliation` | POST /reconcile, job polling, report listing, schema, delta |
| `5 · Full workflow` | Sequential provision → report → reconcile smoke test |
| `6 · Edge cases` | 404s, inverted ranges, empty fields, idempotency of GET |

---

## CI integration

Add the following job to `.github/workflows/ci.yml` (or your equivalent):

```yaml
e2e-testnet:
  name: E2E against Stellar testnet
  runs-on: ubuntu-latest
  needs: [build]
  env:
    API_BASE_URL: http://localhost:8080
    SOROBAN_RPC_URL: https://soroban-testnet.stellar.org
    MANAGER_STELLAR_ADDRESS: ${{ secrets.E2E_STELLAR_ADDRESS }}
    RECONCILE_TIMEOUT_MS: 60000
  steps:
    - uses: actions/checkout@v4

    - name: Start Perigee backend
      run: |
        cd core
        DATABASE_URL=sqlite://e2e.db cargo run &
        # Wait for the server to be ready
        for i in $(seq 1 30); do
          curl -sf http://localhost:8080/health && break
          sleep 2
        done

    - name: Run e2e tests
      run: node --test tests/e2e/api.e2e.test.mjs
```

---

## File structure

```
tests/e2e/
├── .env.example          # Environment variable template
├── api.e2e.test.mjs      # Main test suite (all 6 suites)
├── helpers.mjs           # Shared API client, auth, and assertion helpers
├── package.json          # Optional — lets you run with `npm test`
└── README.md             # This file
```
