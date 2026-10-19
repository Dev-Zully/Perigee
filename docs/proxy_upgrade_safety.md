# Proxy Upgrade Safety

## Problem

The proxy's `upgrade_to` / `upgrade_to_and_call` allowed the admin to point
the proxy at *any* contract address. A compromised or misconfigured admin key
could redirect the proxy (and its authorized `set_storage` key space) to an
arbitrary contract. Upgrading to the already-active implementation was also
an (effective) no-op that silently succeeded.

CONTRACT-31: *ensure admin-only access and revert-on-approve/non-zero-return,
reject self-upgrades.*

## Design

Upgrades are restricted to an admin-managed allowlist of verified
implementations.

| Function | Signature | Returns |
| --- | --- | --- |
| `register_implementation(Env, Address)` | `Result<(), ProxyError>` | `Ok` / `AlreadyVerified` |
| `unregister_implementation(Env, Address)` | `Result<(), ProxyError>` | `Ok` / `NotVerified` |
| `get_verified_implementations(Env)` | `Vec<Address>` | allowlist |
| `upgrade_to(Env, Address)` | `Result<(), ProxyError>` | `Ok` / `NotInitialized` / `UnverifiedImplementation` / `SelfUpgrade` |
| `upgrade_to_and_call(Env, Address, Symbol, Vec<Val>)` | `Result<Val, ProxyError>` | `Ok(Val)` / same errors |

Guards applied, in order, by both upgrade entry points:

1. **Not initialized** — `DataKey::Admin` must exist.
2. **Admin-only** — `admin.require_auth()`.
3. **Self-upgrade rejection** — refusing to re-point at the currently active
   implementation turns a silent no-op into an explicit error.
4. **Verified implementation** — the target must be in the allowlist, which is
   seeded with the implementation passed to `initialize`.

### ABI compatibility

Return types changed from `()` / `Val` to `Result<(), ProxyError>` /
`Result<Val, ProxyError>`. Soroban *client* methods unwrap `Result`, so the
generated client signatures are unchanged (`upgrade_to(&Address) -> ()`,
`upgrade_to_and_call(...) -> Val`); callers migrating from the previous
behavior compile without source changes, while new callers may assert on the
specific error.

### Notes

- `unregister_implementation` can remove the currently active implementation
  from the allowlist; this does not downgrade the proxy, it only blocks future
  upgrades to that address. This is intentional to match existing behavior
  while adding the safety net.
- `delegate_call` and `increment` are unchanged; delegation always goes to the
  (verified at upgrade time) active implementation.

## Verification

`cargo test -p proxy` (7 tests) covers: successful allowlist-registered
upgradability with preserved state, `upgrade_to_and_call` executing the new
implementation's method, rejection of unregistered targets, rejection of
self-upgrades, the same guards on `upgrade_to_and_call`, allowlist removal
blocking future upgrades, and `AlreadyVerified` / `NotVerified` errors.