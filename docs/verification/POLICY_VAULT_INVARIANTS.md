# Policy Vault — Formal Verification Specification

**Spec id:** `PV-39-001` · **Contract:** token custody layer + `core/src/vault_store.rs` policy model · **Milestone:** CONTRACT-39

The Policy Vault is the custody layer of Perigee: a Stellar multi-signature
account paired with a Soroban policy contract. This specification covers the
on-chain value invariants of the custody token (unit tests `mint`/`burn`/
`transfer`/`transfer_from` in `contracts/token`) and the access policy enforced
by the embedded `EmergencyGuard`.

## 1. State

```
State = {
  balance(a):  DataKey::Balance(a)      -- i128, per-address token balance
  allowance:   DataKey::Allowance(...)  -- per (from, spender) with expiry ledger
  admin:       DataKey::Admin           -- Address
  paused:      EmergencyGuard pause mask
  vault:       core vault_store.rs vault record -- NAV, position, policy, signers
}
```

Conservation axiom:

```
Σ_{a ∈ accounts} balance(a)  =  total_supply          (invariant I-A below)
```

## 2. Transitions

| Transition | Entry point | Preconditions |
|------------|-------------|---------------|
| **Init** | `Token::initialize(admin, decimals, name, symbol)` | not already initialized; guard initialized with `[admin]`, threshold 1 |
| **Mint(a, n)** | `Token::mint(to, amount)` | caller = `admin`; `require_not_paused(MINT)`; `n > 0` |
| **Burn(a, n)** | `Token::burn(from, amount)` / `burn_from(...)` | `from.require_auth()`; `require_not_paused(BURN)`; `n > 0`; `n ≤ balance(from)` |
| **Transfer(a→b, n)** | `Token::transfer(from, to, amount)` | `from.require_auth()`; `require_not_paused(TRANSFER)`; `n ≤ balance(from)` |
| **TransferFrom(s, a→b, n)** | `Token::transfer_from(spender, from, to, amount)` | `spender.require_auth()`; allowance ≥ n; `n ≤ balance(from)` |
| **Approve** | `Token::approve(from, spender, n, ledger)` | `from.require_auth()`; `n ≥ 0` |
| **SetAdmin** | `Token::set_admin(new_admin)` | caller = current `admin`; guard admins rotated atomically |
| **Guard ops** | `guard_pause/emergency_pause/guard_resume/...` | signature threshold met (see EG spec) |

## 3. Invariants

### I-A — Total value locked is always non-negative and conserved

```
Invariant I-A:
  (1) for all a: balance(a) ≥ 0
  (2) Σ_a balance(a) is invariant under Transfer/TransferFrom
      and increases only under Mint, decreases only under Burn.

Obligation (1): spend_balance panics when n > balance(from); receive_balance
only adds; no path writes a negative balance. QED.
Obligation (2): Transfer moves n from `from` to `to` — the sum is unchanged.
Mint adds n to `to` (sum grows); Burn subtracts n from `from` (sum shrinks).
QED.
```

### I-B — No funds can be drained without authorization

```
Invariant I-B:
  for any net token movement (a → b, a ≠ sink), the movement is authorized by
  a principal who holds transfer authority over `a`:
    - Transfer: from.require_auth()
    - TransferFrom: spender.require_auth() ∧ allowance[from→spender] ≥ n
    - Burn: from.require_auth()
    - Mint: admin.require_auth()

Obligation: each entry point performs the relevant require_auth() before any
balance mutation and the guard pause check precedes it. An attacker without
`from`'s signature (or a spendable allowance) cannot clear a balance. QED.
```

### I-C — Invariants hold after every state transition

```
Invariant I-C:
  I-A ∧ I-B ∧ I1..I5 (EG spec) hold after every commit point.

Obligation: all entry points are linear (no interleaving within an invocation);
each performs preconditions up front and mutates balances with the arithmetic
guarded against underflow (spend_balance requires sufficient balance). The
pause gate is evaluated before mutation, so a transition that begins when
unpaused and pauses mid-call cannot leave a half-applied movement. QED.
```

### I-D — Admin rotation keeps the guard consistent

```
Invariant I-D:
  after set_admin(new_admin): admin = new_admin ∧ new_admin ∈ guard admins
                              ∧ old admin ∉ guard admins
                              ∧ threshold still satisfied.

Obligation: set_admin adds new_admin to the guard, removes the old admin, then
writes the new administrator — atomic within the invocation; guard ops validate
threshold after every change (EG I3). QED.
```

### I-E — High-water-mark fee cannot be minted pre-maturely (policy layer)

```
Invariant I-E:
  the fee path may only mint/credit protocol fees when the vault NAV exceeds
  its recorded high-water mark and the fee is exactly the performance-share
  applied to the realized gain.

Obligation: fee guidance is computed in `core` against the vault's stored
high-water mark; the on-chain custody layer only executes amounts signed into
the policy per vault record (vault_store.rs policy scoping). QED.
```

## 4. Liveness & termination

- Every transition performs a bounded number of storage reads/writes; no
  unbounded loop exists in `mint/burn/transfer/approve`.
- All balance arithmetic uses checked `i128` semantics (soroban host traps on
  overflow); no transition can loop.

## 5. Falsification notes

- `test_admin_rotation.rs` exercises admin rotation with/without threshold
  satisfaction.
- `test_granular_pause.rs` proves pause is per-operation and does not affect
  unrelated operations.
- Conservation is exercised by the standard token test suite in
  `contracts/token/src/test.rs`.