# TWAP Oracle — Storage Layout Optimization

**CONTRACT-36 · `contracts/twap_oracle`**

## Before

`update_price` touched four separate instance-storage keys on the hot path:

| Key | Access pattern |
|-----|----------------|
| `DataKey::CumulativePrice` | read + write per update |
| `DataKey::TotalTime` | read + write per update |
| `DataKey::LastUpdateTimestamp` | read + write per update |
| `DataKey::LastPrice` | write per update (`read` on first update only via `unwrap_or`) |
| `DataKey::MinUpdateIntervalSeconds` | read per update |

That is **4 writes and 3–4 reads** of instance storage per price update, each
incurring its own storage-frame cost and round trip to the host.

## After

The four accumulator fields are packed into a single
`TwapStorage` struct and stored under one instance key `DataKey::State`:

```rust
#[contracttype]
pub struct TwapStorage {
    pub cumulative_price: i128,       // 16 bytes
    pub total_time: u64,              // 8 bytes
    pub last_update_timestamp: u64,   // 8 bytes
    pub last_price: i128,             // 16 bytes
}
```

`update_price` now performs **1 read + 1 write** of instance storage. `get_twap`
performs **1 read** instead of 2.

## Impact

- **48 bytes before → 48 bytes of payload in one entry + fixed per-key overhead**:
  the per-key encoding + key overhead (~32+ bytes per key) is paid **once**
  instead of four times.
- **Accumulator is decoded/encoded atomically**: a crash or replay can never
  observe a half-updated accumulator (cumulative updated but total_time not),
  which also strengthens the invariant guarantees in
  `docs/verification/EMERGENCY_GUARD_INVARIANTS.md`-style reasoning for the
  oracle.
- `MinUpdateIntervalSeconds`, `TokenA`, and `TokenB` remain write-once keys and
  are unchanged.

## Backward compatibility

This changes the storage layout of the contract. It is a deliberate, gated
change for new deployments; upgrading an existing TWAP oracle would require a
storage migration (out of scope). The public API surface
(`initialize`, `update_price`, `get_twap`, `get_tokens`, `latest_price`) is
unchanged; a new read-only `get_state` accessor was added for transparency.

## Verification

All existing tests in `contracts/twap_oracle/src/test.rs` continue to pass with
identical TWAP math (cumulative 1000+1650=2650, total 10+15=25, TWAP 106).
A new test (`test_state_getter`) asserts the packed state is readable via
`get_state`.