# Per-Contract Deployment Configuration

**Milestone:** CONTRACT-40
**Files:** `scripts/deploy_config/` · `core/src/config.rs`

Each Perigee contract reads its core configuration — admin, initial supply,
time parameters, oracle thresholds — from environment variables at deployment
time instead of hardcoding values. A single validated config drives both the
`soroban` deployment/`initialize` calls and the `Perigee-core` service's
runtime `CONTRACT_*` lookup.

## Layout

```
scripts/deploy_config/
├── contracts.deploy.schema.json   # JSON Schema of the deployment config
├── validate_deployment_config.py  # validator (stdlib only)
└── .env.deploy.example            # reference config you copy to .env.deploy
```

## Usage

### 1. Write your config

Start from the example, fill in real addresses and parameters:

```bash
cp scripts/deploy_config/.env.deploy.example .env.deploy
# edit .env.deploy
```

The validator accepts either an env-style file or the equivalent JSON form
(per the schema):

```bash
# env-style
python3 scripts/deploy_config/validate_deployment_config.py --env-file .env.deploy

# JSON form
python3 scripts/deploy_config/validate_deployment_config.py --config config.json
```

### 2. Emit the deploy environment

To get a shell-safe block you can source right before running deploy scripts:

```bash
eval "$(python3 scripts/deploy_config/validate_deployment_config.py \
         --env-file .env.deploy --emit-env)"
```

This exports:

- `STELLAR_NETWORK` / `APP_ENV` — used by `core/src/config.rs::detect_stage`.
- `CONTRACT_*` variables — resolved by `ContractConfig::from_env()`.
- Per-contract `*_ADMIN`, `*_SUPPLY`, `*_SECONDS`, ... parameters passed
  verbatim to each contract's `initialize` entry point at deploy time.

### 3. Deploy

With the environment sourced, call `soroban contract deploy` for each WASM and
then `soroban contract invoke -- id <id> ... initialize` passing the
per-contract parameters (`--admin $TOKEN_ADMIN`, `--decimal $TOKEN_DECIMALS`,
etc.). See `scripts/deploy_testnet.sh` and `docs/MAINNET_DEPLOYMENT.md` for the
end-to-end flow.

## Contract-to-variable map

| Variable | Contract | Consumed by |
|----------|----------|-------------|
| `CONTRACT_TOKEN` / `TOKEN_ADMIN`, `TOKEN_DECIMALS`, `TOKEN_INITIAL_SUPPLY`, `TOKEN_NAME`, `TOKEN_SYMBOL` | `contracts/token` | `initialize(admin, decimal, name, symbol)` |
| `CONTRACT_EMERGENCY_GUARD` / `GUARD_ADMINS`, `GUARD_THRESHOLD` | `contracts/emergency_guard` | `initialize(admins, threshold)` |
| `CONTRACT_ORACLE_AGGREGATOR` / `ORACLE_MAX_STALENESS_SECONDS` | `contracts/oracle_aggregator` | `initialize(max_staleness)` |
| `CONTRACT_TWAP_ORACLE` / `TWAP_TOKEN_A`, `TWAP_TOKEN_B`, `TWAP_MIN_UPDATE_INTERVAL_SECONDS` | `contracts/twap_oracle` | `initialize(token_a, token_b, min_update_interval_seconds)` |
| `CONTRACT_STAKING_REWARDS` / `STAKING_ADMIN` | `contracts/staking_rewards` | admin wiring |
| `CONTRACT_PROXY` / `PROXY_ADMIN`, `PROXY_INITIAL_IMPLEMENTATION` | `contracts/proxy` | `initialize(admin, implementation)` |
| `CONTRACT_POLICY_VAULT` et al. | core service | `ContractConfig::from_env()` |

## Validation guarantees

The validator enforces:

- Valid Stellar `G…`/`C…` address syntax for every admin/contract id.
- `STELLAR_NETWORK` ∈ {local, testnet, mainnet}; `APP_ENV` ∈ {development,
  staging, production}.
- `TOKEN_DECIMALS` in `[0, 18]`, positive `TOKEN_INITIAL_SUPPLY`, symbol ≤ 12
  chars, name ≤ 32 chars.
- `GUARD_THRESHOLD ≥ 1` and `GUARD_THRESHOLD ≤ |GUARD_ADMINS|`.
- Non-negative oracle staleness, positive TWAP update interval.

Failures exit non-zero so CI or deploy scripts can abort before any on-chain
value is burned by a misconfigured deploy.