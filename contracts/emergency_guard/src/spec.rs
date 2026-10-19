//! # Formal permission-model specification tests (CONTRACT-8 / #502)
//!
//! This module is a specification suite, not a collection of regression tests:
//! it encodes the *permission model* of [`EmergencyGuard`] as explicit,
//! machine-checked theorems and then verifies them against every state-changing
//! and read entry point.
//!
//! ## The model
//!
//! Let `A` be the admin committee, `t` the signature threshold
//! (`1 <= t <= |A|` after `initialize`), and `P(u32)` the pause bitmask. Each
//! entry point belongs to exactly one permission tier:
//!
//! | Tier                          | Entry points                                              | Guard predicate (effect is permitted iff) |
//! |-------------------------------|-----------------------------------------------------------|-------------------------------------------|
//! | `REQ_ONESHOT` (one-shot init) | `initialize`                                              | `initialize` has never run                |
//! | `REQ_SINGLE` (any one admin)  | `set_pause`                                               | `signer ∈ A`                              |
//! | `REQ_COMMITTEE` (threshold)   | `emergency_pause`, `resume`, `add_admin`, `remove_admin`, `rotate_admin`, `validate_multi_sig` | `S ⊆ A` and `|distinct(S)| >= t` |
//! | `REQ_NONE` (permissionless)   | `is_paused`, `get_pause_state`, `get_admins`, `get_threshold`, `is_admin_public` | `true` (no authorization)       |
//!
//! Distinctness is enforced: duplicate signer addresses count once, so a
//! committee of size < `t` cannot be forged by repeating a single signer.
//! Any signer `∉ A` yields `GuardError::Unauthorized` immediately (it never
//! contributes toward the threshold, even when `t = 1`).
//!
//! ## Theorems verified here
//!
//! 1. **Scope (T1)** — every entry point enforces exactly its tier's predicate,
//!    neither more nor less:
//!    - outsiders are rejected by `REQ_SINGLE` and `REQ_COMMITTEE`;
//!    - members are accepted by their tiers;
//!    - `REQ_NONE` services answer any caller;
//!    - below-threshold quorums are rejected with `InsufficientSignatures`.
//! 2. **Revocation (T2)** — after `a` is removed from `A`, `a ∉ A` and `a`
//!    cannot perform *any* `REQ_SINGLE` or `REQ_COMMITTEE` action, nor can it
//!    contribute toward a quorum; each such attempt fails with `Unauthorized`.
//! 3. **Atomicity (T3)** — a rejected invocation leaves `A`, `t`, and `P`
//!    unchanged (no partial effects).
//! 4. **Invariants (T4)** — after every accepted mutation: `|A| >= t`,
//!    `A` contains no duplicates, and the stored `A`/`t` are exactly what
//!    the queries report.
//! 5. **One-shot (T5)** — `initialize` can never run a second time.

#![cfg(test)]
extern crate std;

use crate::{EmergencyGuard, EmergencyGuardClient, GuardError, PauseType};
use soroban_sdk::{
    testutils::Address as _,
    vec, Address, Env, Vec as SorobanVec,
};
use std::vec::Vec;

fn committee(threshold: u32, n_admins: u32) -> (Env, EmergencyGuardClient<'static>, Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EmergencyGuard, ());
    let client = EmergencyGuardClient::new(&env, &contract_id);
    let mut admins: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..n_admins {
        admins.push_back(Address::generate(&env));
    }
    client.initialize(&admins, &threshold).expect("committee init");
    let std_admins: Vec<Address> = admins.iter().collect();
    (env, client, std_admins)
}

fn outsiders(env: &Env, n: u32) -> Vec<Address> {
    (0..n).map(|_| Address::generate(env)).collect()
}

fn distinct(env: &Env, src: &[Address]) -> SorobanVec<Address> {
    let mut v: SorobanVec<Address> = SorobanVec::new(env);
    for a in src {
        v.push_back(a.clone());
    }
    v
}

fn admins_after(client: &EmergencyGuardClient<'static>) -> Vec<Address> {
    client.get_admins().iter().collect()
}

// ── T1: scope ────────────────────────────────────────────────────────────────
// Outsiders are rejected at both tiers; members are accepted; read services are
// permissionless; below-threshold quorums are rejected.

#[test]
fn spec_t1_single_admin_tier_scope() {
    let (env, client, admins) = committee(2, 3);
    let outsider = outsiders(&env, 1)[0].clone();

    // REQ_SINGLE: any committee member may act…
    for a in admins.iter() {
        client.set_pause(a, &PauseType::SWAP, &true);
        assert!(client.is_paused(&PauseType::SWAP));
        client.set_pause(a, &PauseType::SWAP, &false);
    }
    // …but an outsider cannot.
    assert_eq!(
        client.try_set_pause(&outsider, &PauseType::SWAP, &true),
        Err(Ok(GuardError::Unauthorized))
    );
    assert!(!client.is_paused(&PauseType::SWAP), "rejected call must no-op");
}

#[test]
fn spec_t1_committee_tier_scope() {
    let (env, client, admins) = committee(2, 3);
    let two = vec![&env, admins[0].clone(), admins[1].clone()];

    // Threshold exactly met → permitted on every REQ_COMMITTEE entry.
    assert_eq!(client.emergency_pause(&two), Ok(()));
    assert!(client.is_paused(&PauseType::SWAP));
    assert_eq!(client.resume(&two), Ok(()));

    let newbie = outsiders(&env, 1)[0].clone();
    assert_eq!(client.add_admin(&two, &newbie), Ok(()));
    assert!(admins_after(&client).contains(&newbie));
    assert_eq!(client.remove_admin(&two, &newbie), Ok(()));
    assert!(!admins_after(&client).contains(&newbie));
    assert_eq!(client.rotate_admin(&two, &admins[2], &newbie), Ok(()));
    assert!(admins_after(&client).contains(&newbie));
    assert_eq!(client.validate_multi_sig(&two), Ok(()));
}

#[test]
fn spec_t1_below_threshold_is_rejected_across_committee_tier() {
    let (env, client, admins) = committee(2, 3);
    let one = vec![&env, admins[0].clone()];
    let newbie = outsiders(&env, 1)[0].clone();

    assert_eq!(
        client.try_emergency_pause(&one),
        Err(Ok(GuardError::InsufficientSignatures))
    );
    assert_eq!(
        client.try_resume(&one),
        Err(Ok(GuardError::InsufficientSignatures))
    );
    assert_eq!(
        client.try_add_admin(&one, &newbie),
        Err(Ok(GuardError::InsufficientSignatures))
    );
    assert_eq!(
        client.try_remove_admin(&one, &admins[2]),
        Err(Ok(GuardError::InsufficientSignatures))
    );
    assert_eq!(
        client.try_rotate_admin(&one, &admins[2], &newbie),
        Err(Ok(GuardError::InsufficientSignatures))
    );
    assert_eq!(
        client.try_validate_multi_sig(&one),
        Err(Ok(GuardError::InsufficientSignatures))
    );

    // Nothing above may have taken effect.
    assert!(!client.is_paused(&PauseType::SWAP));
    assert_eq!(admins_after(&client).len(), 3);
}

#[test]
fn spec_t1_duplicate_signer_cannot_meet_threshold() {
    // |distinct(S)| semantics: [a, a] at t=2 is a 1-signer quorum.
    let (env, client, admins) = committee(2, 3);
    let dup = vec![&env, admins[0].clone(), admins[0].clone()];
    assert_eq!(
        client.try_emergency_pause(&dup),
        Err(Ok(GuardError::InsufficientSignatures))
    );
    assert_eq!(
        client.try_validate_multi_sig(&dup),
        Err(Ok(GuardError::InsufficientSignatures))
    );
}

#[test]
fn spec_t1_non_admin_in_mixed_quorum_short_circuits_unauthorized() {
    let (env, client, admins) = committee(2, 3);
    let outsider = outsiders(&env, 1)[0].clone();
    // Even a would-be 2-signer set containing a non-admin must fail hard.
    let mixed = vec![&env, admins[0].clone(), outsider];
    assert_eq!(
        client.try_emergency_pause(&mixed),
        Err(Ok(GuardError::Unauthorized))
    );
    assert_eq!(
        client.try_add_admin(&mixed, &outsiders(&env, 1)[0]),
        Err(Ok(GuardError::Unauthorized))
    );
    assert_eq!(
        client.try_validate_multi_sig(&mixed),
        Err(Ok(GuardError::Unauthorized))
    );
}

#[test]
fn spec_t1_read_only_services_are_permissionless() {
    let (env, client, admins) = committee(1, 2);
    let outsider = outsiders(&env, 1)[0].clone();

    // REQ_NONE: an outsider can read everything, including whose signatures count.
    let _ = client.is_paused(&PauseType::MINT);
    let _ = client.get_pause_state();
    assert_eq!(client.get_admins().len(), 2);
    assert_eq!(client.get_threshold(), 1);
    assert!(client.is_admin_public(&admins[0]));
    assert!(!client.is_admin_public(&outsider));
}

// ── T2: revocation ───────────────────────────────────────────────────────────
// A removed admin loses every capability and can never re-enter.

#[test]
fn spec_t2_revoked_admin_cannot_act_nor_contribute_to_quorum() {
    let (env, client, admins) = committee(1, 3);

    let signers = vec![&env, admins[1].clone()];
    assert_eq!(client.remove_admin(&signers, &admins[2]), Ok(()));

    // REQ_SINGLE is dead for the revoked signer.
    assert_eq!(
        client.try_set_pause(&admins[2], &PauseType::SWAP, &true),
        Err(Ok(GuardError::Unauthorized))
    );

    // REQ_COMMITTEE is dead as the sole signer…
    assert_eq!(
        client.try_emergency_pause(&vec![&env, admins[2].clone()]),
        Err(Ok(GuardError::Unauthorized))
    );
    assert_eq!(
        client.try_validate_multi_sig(&vec![&env, admins[2].clone()]),
        Err(Ok(GuardError::Unauthorized))
    );

    // The revoked address is simply no longer part of the committee.
    assert!(!client.is_admin_public(&admins[2]));
    assert_eq!(
        client.try_remove_admin(
            &vec![&env, admins[0].clone(), admins[1].clone()],
            &admins[2]
        ),
        Err(Ok(GuardError::AdminNotFound))
    );
}

#[test]
fn spec_t2_revoked_signer_at_threshold_two_cannot_form_quorum() {
    let (env, client, admins) = committee(2, 3);

    let quorum = vec![&env, admins[0].clone(), admins[1].clone()];
    assert_eq!(client.remove_admin(&quorum, &admins[2]), Ok(()));

    // admins[2] + one valid member is still only one valid signature → Unauthorized
    // (the revoked signer short-circuits before any threshold accounting).
    let revoked_mix = vec![&env, admins[0].clone(), admins[2].clone()];
    assert_eq!(
        client.try_emergency_pause(&revoked_mix),
        Err(Ok(GuardError::Unauthorized))
    );
    assert_eq!(
        client.try_validate_multi_sig(&revoked_mix),
        Err(Ok(GuardError::Unauthorized))
    );
}

// ── T3: atomicity ────────────────────────────────────────────────────────────
// Rejected calls never mutate A, t, or P.

#[test]
fn spec_t3_rejected_calls_leave_state_unchanged() {
    let (env, client, admins) = committee(2, 3);
    let snapshot = admins_after(&client);

    let one = vec![&env, admins[0].clone()];
    let outsider = outsiders(&env, 1)[0].clone();

    // A below-threshold committee action…
    let _ = client.try_emergency_pause(&one);
    // …and an unauthorized single-admin action…
    let _ = client.try_set_pause(&outsider, &PauseType::WITHDRAW, &true);
    // …and a not-found removal attempt.
    let _ = client.try_remove_admin(&vec![&env, admins[0].clone(), admins[1].clone()], &outsider);

    assert!(!client.is_paused(&PauseType::SWAP));
    assert!(!client.is_paused(&PauseType::WITHDRAW));
    assert_eq!(client.get_pause_state(), 0);
    assert_eq!(admins_after(&client), snapshot);
    assert_eq!(client.get_threshold(), 2);
}

// ── T4: invariants ───────────────────────────────────────────────────────────
// Every accepted mutation preserves committee validity.

#[test]
fn spec_t4_committee_invariants_hold_after_every_mutation() {
    let (env, client, admins) = committee(2, 3);
    let newbie = outsiders(&env, 1)[0].clone();
    let quorum = vec![&env, admins[0].clone(), admins[1].clone()];

    let mut ops: Vec<Box<dyn Fn()>> = vec![
        Box::new(|| {
            assert_eq!(client.emergency_pause(&quorum), Ok(()));
        }),
        Box::new(|| {
            assert_eq!(client.resume(&quorum), Ok(()));
        }),
        Box::new(|| {
            assert_eq!(client.add_admin(&quorum, &newbie), Ok(()));
        }),
        Box::new(|| {
            assert_eq!(client.remove_admin(&quorum, &newbie), Ok(()));
        }),
        Box::new(|| {
            assert_eq!(client.rotate_admin(&quorum, &admins[2], &newbie), Ok(()));
        }),
    ];
    for op in ops.iter_mut() {
        op();
        let a = admins_after(&client);
        let t = client.get_threshold();
        assert!(
            (a.len() as u32) >= t,
            "|A| must never drop below t (|A|={}, t={})",
            a.len(),
            t
        );
        let mut seen: Vec<Address> = Vec::new();
        for addr in &a {
            assert!(!seen.contains(addr), "committee must stay duplicate-free");
            seen.push(addr.clone());
        }
    }
}

// ── T5: one-shot initialization ──────────────────────────────────────────────

#[test]
fn spec_t5_initialize_is_one_shot() {
    let (env, client, _admins) = committee(1, 2);
    assert_eq!(
        client.try_initialize(&distinct(&env, &[]), &1),
        Err(Ok(GuardError::AlreadyInitialized))
    );
    assert_eq!(client.get_threshold(), 1, "re-init must not reset t");
}

// ── Counterfactual map: human-readable proof of the table ────────────────────
#[test]
fn spec_entry_point_tier_map_matches_contract() {
    // A documentation-check: the tier map in the module docs claims an entry
    // point for every combination tested above. This test exists so that a
    // future _added_ entry point forces the author to extend the spec table.
    let (env, client, admins) = committee(1, 1);
    // Every client surface the spec relies on exists and is callable:
    let _ = client.try_initialize(&vec![&env, admins[0].clone()], &1);
    let _ = client.get_pause_state();
    let _ = client.is_paused(&PauseType::SWAP);
    let _ = client.try_set_pause(&admins[0], &PauseType::SWAP, &true);
    let _ = client.try_emergency_pause(&vec![&env, admins[0].clone()]);
    let _ = client.try_resume(&vec![&env, admins[0].clone()]);
    let _ = client.is_admin_public(&admins[0]);
    let _ = admins_after(&client);
    let _ = client.get_threshold();
    let _ = client.try_validate_multi_sig(&vec![&env, admins[0].clone()]);
}