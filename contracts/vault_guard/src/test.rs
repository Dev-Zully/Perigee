#![cfg(test)]

//! Unit tests for the vault guard.
//!
//! Covers the four controls #634 names — circuit breaker, admin pause, timelocked
//! unpause, partial pause — and the separation property that makes the circuit
//! breaker worth having: the guardian is not the admin.
//!
//! Closes OdyxeeeLabs/Perigee#634

use crate::{
    Error, Operation, VaultGuard, VaultGuardClient, OP_ALL, OP_DEPOSIT, OP_FEE_CHANGE,
    OP_UPGRADE, OP_WITHDRAW, UNPAUSE_DELAY_LEDGERS,
};
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

macro_rules! setup {
    () => {{
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| li.sequence_number = 1_000);

        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);

        let contract_id = env.register(VaultGuard, ());
        let client = VaultGuardClient::new(&env, &contract_id);
        client.initialize(&admin, &guardian);

        (env, contract_id, client, admin, guardian)
    }};
}

// ── Initialization ───────────────────────────────────────────────────────────

#[test]
fn initialization_records_distinct_roles() {
    let (env, _cid, client, admin, guardian) = setup!();

    let roles = client.roles();
    assert_eq!(roles.admin, admin);
    assert_eq!(roles.guardian, guardian);
    assert!(!client.is_globally_paused());
    assert_eq!(client.paused_mask(), 0);
}

#[test]
fn the_guardian_may_not_also_be_the_admin() {
    // A combined role would let the admin trip the breaker with no timelock,
    // which is exactly the separation the breaker exists to provide.
    let env = Env::default();
    env.mock_all_auths();

    let same = Address::generate(&env);
    let contract_id = env.register(VaultGuard, ());
    let client = VaultGuardClient::new(&env, &contract_id);

    assert_eq!(
        client.try_initialize(&same, &same),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn the_guard_can_only_be_initialized_once() {
    let (env, _cid, client, admin, guardian) = setup!();

    assert_eq!(
        client.try_initialize(&admin, &guardian),
        Err(Ok(Error::AlreadyInitialized))
    );
}

// ── Admin pause ──────────────────────────────────────────────────────────────

#[test]
fn admin_pause_stops_everything() {
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause(&admin);

    assert!(client.is_globally_paused());
    assert_eq!(client.paused_mask(), OP_ALL);
    for operation in [
        Operation::Deposit,
        Operation::Withdraw,
        Operation::Rotate,
        Operation::FeeChange,
        Operation::Upgrade,
    ] {
        assert!(client.is_paused(&operation));
    }
}

#[test]
fn pausing_twice_is_refused() {
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause(&admin);
    assert_eq!(client.try_pause(&admin), Err(Ok(Error::AlreadyPaused)));
}

#[test]
fn only_the_admin_may_pause() {
    let (env, _cid, client, _admin, guardian) = setup!();

    assert_eq!(
        client.try_pause(&guardian),
        Err(Ok(Error::Unauthorized))
    );
    assert!(!client.is_globally_paused());
}

// ── Partial pause ────────────────────────────────────────────────────────────

#[test]
fn a_partial_pause_leaves_other_operations_running() {
    // The incident that motivates partial pause: a deposit-drain exploit does
    // not touch withdrawals, and pausing everything locks real users out of
    // their funds.
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause_operations(&admin, &OP_DEPOSIT);

    assert!(client.is_paused(&Operation::Deposit));
    assert!(!client.is_paused(&Operation::Withdraw));
    assert!(!client.is_paused(&Operation::Rotate));
    assert_eq!(client.paused_mask(), OP_DEPOSIT);
    assert!(!client.is_globally_paused());
}

#[test]
fn partial_pauses_accumulate() {
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause_operations(&admin, &OP_DEPOSIT);
    client.pause_operations(&admin, &crate::OP_ROTATE);

    assert_eq!(client.paused_mask(), OP_DEPOSIT | crate::OP_ROTATE);
}


#[test]
fn an_unknown_operation_bit_is_rejected() {
    // A mistyped mask must fail loudly rather than appearing to cover an
    // operation it does not.
    let (env, _cid, client, admin, _guardian) = setup!();

    assert_eq!(
        client.try_pause_operations(&admin, &(1 << 9)),
        Err(Ok(Error::UnknownOperation))
    );
    assert_eq!(client.paused_mask(), 0);
}

#[test]
fn re_pausing_the_same_operations_is_refused() {
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause_operations(&admin, &OP_DEPOSIT);
    assert_eq!(
        client.try_pause_operations(&admin, &OP_DEPOSIT),
        Err(Ok(Error::AlreadyPaused))
    );
}

#[test]
fn resuming_an_operation_that_is_not_paused_is_refused() {
    let (env, _cid, client, admin, _guardian) = setup!();

    assert_eq!(
        client.try_resume_operations(&admin, &OP_DEPOSIT),
        Err(Ok(Error::OperationNotPaused))
    );
}

#[test]
fn individual_operations_resume_without_waiting_for_the_timelock() {
    // Only lifting a *global* pause stops every user, so only that is timelocked.
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause_operations(&admin, &(OP_DEPOSIT | OP_WITHDRAW));
    client.resume_operations(&admin, &OP_WITHDRAW);

    assert!(client.is_paused(&Operation::Deposit));
    assert!(!client.is_paused(&Operation::Withdraw));
    // Resuming the last one clears the pause clock entirely.
    client.resume_operations(&admin, &OP_DEPOSIT);
    assert_eq!(client.paused_mask(), 0);
    assert_eq!(client.unpause_delay_remaining(), 0);
}

#[test]
fn the_global_pause_overrides_a_partial_mask() {
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause_operations(&admin, &OP_DEPOSIT);
    client.pause(&admin);

    // Even an operation outside the mask is stopped while the global pause holds.
    assert!(client.is_paused(&Operation::Withdraw));
    assert_eq!(client.paused_mask(), OP_ALL);
}

// ── Timelocked unpause ───────────────────────────────────────────────────────

#[test]
fn unpausing_too_early_is_refused() {
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause(&admin);
    assert_eq!(
        client.try_unpause(&admin),
        Err(Ok(Error::UnpauseTooEarly))
    );
    assert!(client.is_globally_paused());
    assert_eq!(client.unpause_delay_remaining(), UNPAUSE_DELAY_LEDGERS);
}

#[test]
fn unpause_succeeds_once_the_window_has_elapsed() {
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause(&admin);
    env.ledger()
        .with_mut(|li| li.sequence_number += UNPAUSE_DELAY_LEDGERS);

    client.unpause(&admin);

    assert!(!client.is_globally_paused());
    assert_eq!(client.unpause_delay_remaining(), 0);
}

#[test]
fn anyone_may_lift_the_pause_once_the_window_has_elapsed() {
    // The gate is time, not identity: a timelock the admin can shorten is not a
    // timelock.
    let (env, _cid, client, admin, _guardian) = setup!();

    client.pause(&admin);
    env.ledger()
        .with_mut(|li| li.sequence_number += UNPAUSE_DELAY_LEDGERS);

    let bystander = Address::generate(&env);
    client.unpause(&bystander);

    assert!(!client.is_globally_paused());
}

#[test]
fn unpausing_when_not_paused_is_refused() {
    let (env, _cid, client, admin, _guardian) = setup!();

    assert_eq!(client.try_unpause(&admin), Err(Ok(Error::NotPaused)));
}

#[test]
fn the_delay_is_reported() {
    let (_env, _cid, client, _admin, _guardian) = setup!();

    assert_eq!(client.unpause_delay(), UNPAUSE_DELAY_LEDGERS);
}

// ── Circuit breaker ──────────────────────────────────────────────────────────

#[test]
fn the_guardian_can_trip_the_breaker_without_a_timelock() {
    let (env, _cid, client, _admin, guardian) = setup!();

    client.trip(&guardian);

    assert!(client.is_tripped());
    assert!(client.is_globally_paused());
    assert_eq!(client.state().tripped_by, Some(guardian));
    // No delay: the breaker is the control for when waiting is not acceptable.
    assert_eq!(client.unpause_delay_remaining(), UNPAUSE_DELAY_LEDGERS);
}

#[test]
fn the_admin_cannot_trip_the_breaker() {
    // The separation is the point: a compromised admin must not be able to halt
    // the protocol through the one control that has no timelock.
    let (env, _cid, client, admin, _guardian) = setup!();

    assert_eq!(client.try_trip(&admin), Err(Ok(Error::NotGuardian)));
    assert!(!client.is_tripped());
}

#[test]
fn the_breaker_cannot_be_tripped_twice() {
    let (env, _cid, client, _admin, guardian) = setup!();

    client.trip(&guardian);
    assert_eq!(client.try_trip(&guardian), Err(Ok(Error::AlreadyTripped)));
}

#[test]
fn a_trip_still_has_to_wait_out_the_unpause_delay() {
    // The guardian can stop the vault immediately but cannot keep it halted:
    // the way out is timelocked and observable.
    let (env, _cid, client, _admin, guardian) = setup!();

    client.trip(&guardian);
    assert_eq!(client.try_unpause(&guardian), Err(Ok(Error::UnpauseTooEarly)));

    env.ledger()
        .with_mut(|li| li.sequence_number += UNPAUSE_DELAY_LEDGERS);
    client.unpause(&guardian);

    assert!(!client.is_globally_paused());
    assert!(client.is_tripped(), "the trip flag survives the unpause");
}

#[test]
fn the_trip_flag_is_reset_only_after_the_pause_is_lifted() {
    let (env, _cid, client, _admin, guardian) = setup!();

    client.trip(&guardian);

    // Cannot reset while the incident is still in progress.
    assert_eq!(
        client.try_reset_trip(&guardian),
        Err(Ok(Error::AlreadyPaused))
    );

    env.ledger()
        .with_mut(|li| li.sequence_number += UNPAUSE_DELAY_LEDGERS);
    client.unpause(&guardian);
    client.reset_trip(&guardian);

    assert!(!client.is_tripped());
    assert_eq!(client.state().tripped_by, None);
}

#[test]
fn resetting_an_untripped_guard_is_refused() {
    let (env, _cid, client, _admin, guardian) = setup!();

    assert_eq!(client.try_reset_trip(&guardian), Err(Ok(Error::NotPaused)));
}

#[test]
fn only_the_guardian_may_reset_the_trip() {
    let (env, _cid, client, admin, guardian) = setup!();

    client.trip(&guardian);
    env.ledger()
        .with_mut(|li| li.sequence_number += UNPAUSE_DELAY_LEDGERS);
    client.unpause(&guardian);

    assert_eq!(client.try_reset_trip(&admin), Err(Ok(Error::NotGuardian)));
}

// ── Operation bits ───────────────────────────────────────────────────────────

#[test]
fn operation_bits_are_distinct_and_within_the_mask() {
    let bits = [
        Operation::Deposit.bit(),
        Operation::Withdraw.bit(),
        Operation::Rotate.bit(),
        Operation::FeeChange.bit(),
        Operation::Upgrade.bit(),
    ];

    for (index, bit) in bits.iter().enumerate() {
        assert!(bit.is_power_of_two(), "bit {index} must be a single bit");
        assert!(bit & OP_ALL != 0, "bit {index} must be within OP_ALL");
        for other in bits.iter().skip(index + 1) {
            assert_ne!(bit, other, "bits {index} and the later one collide");
        }
    }
    assert_eq!(bits.iter().fold(0, |acc, bit| acc | bit), OP_ALL);
}

#[test]
fn the_remaining_delay_counts_down_and_reaches_zero() {
    let (env, _cid, client, admin, _guardian) = setup!();

    assert_eq!(client.unpause_delay_remaining(), 0);

    client.pause(&admin);
    assert_eq!(client.unpause_delay_remaining(), UNPAUSE_DELAY_LEDGERS);

    env.ledger()
        .with_mut(|li| li.sequence_number += UNPAUSE_DELAY_LEDGERS / 2);
    assert_eq!(client.unpause_delay_remaining(), UNPAUSE_DELAY_LEDGERS / 2);

    env.ledger()
        .with_mut(|li| li.sequence_number += UNPAUSE_DELAY_LEDGERS / 2);
    client.unpause(&admin);

    assert_eq!(client.unpause_delay_remaining(), 0);
}