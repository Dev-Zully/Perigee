#![cfg(test)]

use crate::{Error, FeeSwitch, FeeSwitchClient, FeeSwitchConfig, BPS_DENOMINATOR};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, Address, Address, FeeSwitchConfig) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let config = FeeSwitchConfig {
        fee_bps: 30,
        treasury_bps: 5_000,
        stakers_bps: 3_000,
        buyback_bps: 2_000,
        treasury: Address::generate(&env),
        stakers_recipient: Address::generate(&env),
        governance_token: Address::generate(&env),
    };
    let id = env.register(FeeSwitch, ());
    let client = FeeSwitchClient::new(&env, &id);
    client.initialize(&admin, &config);
    (env, id, admin, config)
}

#[test]
fn starts_inactive_and_can_be_toggled() {
    let (env, id, admin, _) = setup();
    let client = FeeSwitchClient::new(&env, &id);
    assert!(!client.is_active());
    client.activate(&admin);
    assert!(client.is_active());
    client.deactivate(&admin);
    assert!(!client.is_active());
}

#[test]
fn collect_requires_active_switch() {
    let (env, id, _admin, _) = setup();
    let client = FeeSwitchClient::new(&env, &id);
    assert_eq!(client.try_collect(&1_000), Err(Ok(Error::SwitchInactive)));
}

#[test]
fn collect_splits_revenue_exactly() {
    let (env, id, admin, _) = setup();
    let client = FeeSwitchClient::new(&env, &id);
    client.activate(&admin);
    let buckets = client.collect(&10_000);
    assert_eq!(buckets.treasury, 5_000);
    assert_eq!(buckets.stakers, 3_000);
    assert_eq!(buckets.buyback, 2_000);
    assert_eq!(
        buckets.treasury + buckets.stakers + buckets.buyback,
        buckets.total_collected
    );
}

#[test]
fn buyback_burns_and_reduces_budget() {
    let (env, id, admin, _) = setup();
    let client = FeeSwitchClient::new(&env, &id);
    client.activate(&admin);
    client.collect(&10_000);
    let after = client.execute_buyback(&admin, &1_000, &50);
    assert_eq!(after.buyback, 1_000);
    assert_eq!(after.total_burned, 50);
}

#[test]
fn invalid_bps_split_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(FeeSwitch, ());
    let client = FeeSwitchClient::new(&env, &id);
    let bad = FeeSwitchConfig {
        fee_bps: 30,
        treasury_bps: 4_000,
        stakers_bps: 3_000,
        buyback_bps: 2_000, // sums to 9_000
        treasury: Address::generate(&env),
        stakers_recipient: Address::generate(&env),
        governance_token: Address::generate(&env),
    };
    assert_eq!(
        client.try_initialize(&admin, &bad),
        Err(Ok(Error::InvalidBps))
    );
    let _ = BPS_DENOMINATOR;
}

#[test]
fn quote_fee_zero_when_inactive() {
    let (env, id, admin, _) = setup();
    let client = FeeSwitchClient::new(&env, &id);
    assert_eq!(client.quote_fee(&1_000_000), 0);
    client.activate(&admin);
    assert_eq!(client.quote_fee(&1_000_000), 3_000); // 30 bps
}
