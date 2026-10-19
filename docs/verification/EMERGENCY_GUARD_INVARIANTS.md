# Emergency Guard — Formal Verification Specification

**Spec id:** `EG-39-001` · **Contract:** `contracts/emergency_guard` · **Milestone:** CONTRACT-39

The Emergency Guard is a reusable multi-signature circuit breaker embedded in
every protected contract (token, vaults, oracles). It gates state-changing
operations behind an explicit pause bit and requires a `threshold`-of-`admins`
approver set for administrative actions. This specification proves its core
invariants hold across every transition.

## 1. State

```
State = {
  paused:       GuardDataKey::PauseState        -- bit-flagged pause mask (u32)
  admins:       GuardDataKey::Admins            -- Vec<Address>
  threshold:    GuardDataKey::SignatureThreshold -- u32
}
```

### Derived invariants of the state encoding

- `admins` is finite and has no duplicate entries.
- `1 ≤ threshold ≤ |admins|` whenever `admins` is non-empty (established by
  `initialize` and preserved by `add_admin` / `remove_admin`, which enforce
  `1 ≤ threshold ≤ |admins|` after mutation).
- `paused` is a bitmask: every bit `op` is either `0` (unpaused) or `1`
  (paused). No other values are writeable — all transitions write
  `PauseType::set_paused` results, which only toggle individual bits.

## 2. Transitions

| Transition | Entry point | Guards |
|------------|-------------|--------|
| **Init** | `EmergencyGuard::initialize(admins, threshold)` | `admins` non-empty, valid `threshold`; can only run once (`GuardError::AlreadyInitialized`) |
| **SetPause(op, paused)** | `EmergencyGuard::set_pause(admin, op, paused)` | `admin ∈ admins`; caller authenticates |
| **EmergencyPause(all)** | `EmergencyGuard::emergency_pause(approvers)` | `approvers` meets `threshold`-of-`admins` |
| **Resume** | `EmergencyGuard::resume(approvers)` | `approvers` meets `threshold`-of-`admins` |
| **AddAdmin(a)** | `EmergencyGuard::add_admin(approvers, a)` | signer set satisfies threshold |
| **RemoveAdmin(a)** | `EmergencyGuard::remove_admin(approvers, a)` | signer set satisfies threshold; final admin cannot be removed below threshold |

Every transition is paired with a documented event emission
(`emergency_guard_*` topics in `contracts/emergency_guard/src/lib.rs`).

## 3. Invariants

### I1 — Paused ⇒ blocked (no unauthorized state change through the guard)

```
Invariant I1:
  for all ops, if the guard's pause bit for `op` is set
  then no protected entry point gated by `require_not_paused(op)` may change
  the guarded asset state.

Step-proof obligation:
  For transitions SetPause(op, true): the guarded op's own entry point
  (`token::mint`, `token::burn`, `token::transfer`, ...) checks
  `require_not_paused(op)` which panics when the bit is set, so the guarded
  operation cannot run. QED.
```

### I2 — Signature threshold cannot be violated

```
Invariant I2:
  for every administrative transition, the set of authenticated approvers A
  satisfies: |A ∩ admins| ≥ threshold  before the transition commits.

Obligation:
  add_admin/remove_admin/emergency_pause/resume verify the signature count
  against the currently stored threshold and return
  GuardError::InsufficientSignatures otherwise. Because the check reads the
  same storage the mutation writes, there is no TOCTOU window within a single
  invocation. QED.
```

### I3 — Threshold never exceeds the admin set

```
Invariant I3:
  threshold ≤ |admins|  at every reachable state.

Obligation:
  Init: enforce 1 ≤ threshold ≤ |admins|.
  AddAdmin(a): |admins| grows by 1, threshold unchanged, so threshold ≤ |admins|.
  RemoveAdmin(a): the contract rejects removal when the resulting |admins|
  would fall below threshold (GuardError::InvalidThreshold). QED.
```

### I4 — Only admins may change the guard

```
Invariant I4:
  for every transition that mutates `admins`, `threshold`, or `paused`,
  the caller/approver set is drawn from `admins` and satisfies `threshold`.

Obligation:
  set_pause requires `from.require_auth()` where `from ∈ admins`;
  the multi-sig operations require the stored threshold satisfaction.
  Non-admin callers therefore cannot mutate guard state. QED.
```

### I5 — No re-initialization

```
Invariant I5:
  the guard can be initialized at most once.

Obligation:
  initialize checks `GuardDataKey::Admins` presence and returns
  GuardError::AlreadyInitialized before writing anything. QED.
```

## 4. Liveness & termination

- All loops in `emergency_guard` iterate over the stored `admins` list or the
  approver list, both finite; every transition terminates.
- Publication of the corresponding event occurs after the state write, so a
  state change is always observable via the event stream (event-indexing hook).

## 5. Falsification notes (manually validated in `src/test.rs`)

- Multi-sig pause with insufficient approvers → `InsufficientSignatures`.
- Pause/unpause toggling preserves the other operation bits.
- Admin removal that would break the threshold is rejected.