# Formal Verification Specifications

This directory contains formal verification specifications for the most
critical contracts in the Perigee protocol. Each specification describes the
contract's *state*, its *transition functions* (mapped to the actual on-chain
entry points), and the *invariant properties* that must hold before and after
every state transition.

These specifications are written in a TLA+-style predicate notation so they are
machine-checkable in principle:

- `∀`, `∃`, `⇒`, `⇔` — universal / existential quantification, implication, equivalence
- `∧`, `∨`, `¬` — conjunction, disjunction, negation
- `S ⟶ S'` — a transition from state `S` to successor state `S'`
- `Init(S)` — the initial-state predicate (holding after `initialize`)
- `Invariant(S) ⇒ Invariant(S')` — step-proof obligation for every transition

## Index

| Spec | Coverage | Entry points verified |
|------|----------|------------------------|
| [Emergency Guard](./EMERGENCY_GUARD_INVARIANTS.md) | Multi-sig pause guard used by every protected contract | `initialize`, `set_pause`, `emergency_pause`, `resume`, `add_admin`, `remove_admin`, `is_paused` |
| [Policy Vault](./POLICY_VAULT_INVARIANTS.md) | Custody + fee-settlement layer (token mint/burn/transfer, high-water-mark fee accrual) | `initialize`, `mint`, `burn`, `transfer`, `transfer_from`, `set_admin`, guard operations |

## How to review

1. Read `Init(S)` (state that must hold immediately after deployment).
2. For every transition `S ⟶ S'`, confirm each entry point it maps to enforces
   the stated preconditions (`Pre`) and that `Invariant(S) ⇒ Invariant(S')` is
   therefore preserved.
3. Report any transition for which the step-proof obligation does **not** hold.

## Mapping to repository code

Every predicate references the concrete storage keys and functions in the
`contracts/` tree. For example, "paused-ness" maps to the instance-storage key
`GuardDataKey::PauseState` (`contracts/emergency_guard/src/lib.rs`) and the
`require_not_paused` gate in `contracts/token/src/contract.rs`.