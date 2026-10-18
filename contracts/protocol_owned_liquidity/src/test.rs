#![cfg(test)]

use crate::{
    quote_bond_tokens, Error, PolConfig, ProtocolOwnedLiquidity, ProtocolOwnedLiquidityClient,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(ProtocolOwnedLiquidity, ());
    let client = ProtocolOwnedLiquidityClient::new(&env, &id);
    client.initialize(
        &admin,
        &PolConfig {
            governance_token: Address::generate(&env),
            treasury: Address::generate(&env),
            discount_bps: 500, // 5%
            vesting_ledgers: 10,
            max_bond_liability: 1_000_000,
        },
    );
    (env, id, admin)
}

#[test]
fn bond_pricing_applies_discount() {
    // deposit 10_000 value, price 10_000 (1:1), 5% discount → 10_500 tokens
    let tokens = quote_bond_tokens(10_000, 10_000, 500).unwrap();
    assert_eq!(tokens, 10_500);
}

#[test]
fn bond_and_claim_after_vesting() {
    let (env, id, _) = setup();
    let client = ProtocolOwnedLiquidityClient::new(&env, &id);
    let bonder = Address::generate(&env);

    let (bond_id, tokens) = client.bond(&bonder, &10_000, &10_000);
    assert_eq!(tokens, 10_500);
    assert_eq!(client.get_state().reserves, 10_000);
    assert_eq!(client.get_state().bond_liability, 10_500);

    assert_eq!(
        client.try_claim_bond(&bond_id, &bonder),
        Err(Ok(Error::BondNotMature))
    );

    env.ledger().with_mut(|l| l.sequence_number += 10);
    let claimed = client.claim_bond(&bond_id, &bonder);
    assert_eq!(claimed, 10_500);
    assert_eq!(client.get_state().bond_liability, 0);
}

#[test]
fn deploy_pol_and_capture_yield() {
    let (env, id, admin) = setup();
    let client = ProtocolOwnedLiquidityClient::new(&env, &id);
    let bonder = Address::generate(&env);
    client.bond(&bonder, &50_000, &10_000);

    let state = client.deploy_pol(&admin, &20_000, &100);
    assert_eq!(state.reserves, 30_000);
    assert_eq!(state.deployed_lp, 100);

    client.capture_yield(&admin, &1_500);
    let swept = client.sweep_yield(&admin);
    assert_eq!(swept, 1_500);
    assert_eq!(client.get_state().yield_pending, 0);
    assert_eq!(client.get_state().reserves, 31_500);
    assert_eq!(client.get_state().yield_captured, 1_500);
}

#[test]
fn cannot_deploy_more_than_reserves() {
    let (env, id, admin) = setup();
    let client = ProtocolOwnedLiquidityClient::new(&env, &id);
    assert_eq!(
        client.try_deploy_pol(&admin, &1, &1),
        Err(Ok(Error::InsufficientReserves))
    );
}
