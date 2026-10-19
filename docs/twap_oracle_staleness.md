# TWAP Oracle Staleness Checks

## Problem

Consumers of the TWAP oracle had no way to know whether the last price update
was recent. A feed that stopped being updated would keep returning a cached
average, and downstream logic (e.g. the oracle aggregator) could price trades
off a dead feed.

CONTRACT-33: *implement oracle price staleness checks before using price
feeds.*

## Design

The staleness check is additive and ABI-preserving:

| Function | Signature | Behavior |
| --- | --- | --- |
| `set_max_price_age(Env, u64)` | `Result<(), Error>` | Configures the maximum age in seconds. `0` (the default) disables the check. Returns `Error::NotInitialized` before `initialize`. |
| `get_max_price_age(Env)` | `u64` | Returns the configured maximum age. |
| `is_price_stale(Env)` | `bool` | `true` when the last update is older than the window, or when no price has ever been recorded. Always `false` while checks are disabled. |
| `latest_price(Env)` | `i128` | Returns `0` while the feed is stale so consumers treat the quote as unavailable. |

Guards applied in `is_price_stale`:

1. Checks disabled (`max_age_seconds == 0`) → `false`.
2. No update ever recorded (`LastUpdateTimestamp == 0`) → `true`, so an empty
   feed is never trusted.
3. Otherwise `now.saturating_sub(last_update) > max_age_seconds` → `true`.

## Rationale

- `latest_price` keeps its `i128` ABI. The oracle aggregator (and other
  consumers) already treat a `0` quote as "no reliable value", so returning
  `0` for a stale feed composes safely with existing integration logic.
- The check is opt-in. Deployments that do not call `set_max_price_age`
  observe exactly the previous behavior.
- A separate config entry (rather than changing `initialize`) preserves the
  deploy-time ABI for already-live oracle instances.

## Verification

`cargo test -p twap_oracle` (9 tests) covers: default-disabled behavior, the
exact staleness boundary, revival after a fresh update, a never-updated feed,
and re-enabling/disabling the check at runtime.