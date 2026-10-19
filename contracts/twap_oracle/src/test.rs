#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

use crate::{Error, TwapOracle, TwapOracleClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup_pair(e: &Env) -> (Address, Address) {
    let token_a = Address::generate(e);
    let token_b = Address::generate(e);
    (token_a, token_b)
}

#[test]
fn test_initialize() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    assert_eq!(client.initialize(&token_a, &token_b, &60), ());
    assert_eq!(client.get_tokens(), (token_a, token_b));
    assert_eq!(client.get_max_price_age(), 0);
}

#[test]
fn test_not_initialized() {
    let e = Env::default();
    let contract_id = e.register(TwapOracle, ());
    let token_a = Address::generate(&e);
    let token_b = Address::generate(&e);

    client.initialize(&token_a, &token_b, &60);

    // Try to initialize again (direct call so the Err is observable).
    let result = e.as_contract(&contract_id, || {
        TwapOracle::initialize(e.clone(), token_a.clone(), token_b.clone(), 60)
    });
    assert_eq!(result, Err(Error::AlreadyInitialized));

    assert_eq!(
        e.as_contract(&contract_id, || TwapOracle::update_price(e.clone(), 100)),
        Err(Error::NotInitialized)
    );
    assert_eq!(
        e.as_contract(&contract_id, || TwapOracle::set_max_price_age(e.clone(), 60)),
        Err(Error::NotInitialized)
    );
}

#[test]
fn test_invalid_price() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    client.initialize(&token_a, &token_b, &60);

    assert_eq!(
        e.as_contract(&contract_id, || TwapOracle::update_price(e.clone(), 0)),
        Err(Error::InvalidPrice)
    );
    assert_eq!(
        e.as_contract(&contract_id, || TwapOracle::update_price(e.clone(), -5)),
        Err(Error::InvalidPrice)
    );
}
    // First update
    client.update_price(&100);
    assert_eq!(client.get_twap(), 0); // No time elapsed yet

#[test]
fn test_update_and_get_twap() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    client.initialize(&token_a, &token_b, &1);
    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.update_price(&90);
    // Second update
    client.update_price(&110);
    // TWAP = (100 * 10) / 10 = 100
    assert_eq!(client.get_twap(), 100);

    // The first update seeds the accumulator; later updates weight prior
    // prices over their elapsed window.
    e.ledger().with_mut(|li| li.timestamp = 1100);
    client.update_price(&100);
    e.ledger().with_mut(|li| li.timestamp = 1200);
    client.update_price(&200);

    // cumulative = 90*100 + 100*100 = 19000 over total time 200 => 95.
    assert_eq!(client.get_twap(), 95);
    assert_eq!(client.latest_price(), 95);
    // Third update
    client.update_price(&120);
    // Cumulative = 100*10 + 110*15 = 1000 + 1650 = 2650
    // Total time = 10 + 15 = 25
    // TWAP = 2650 / 25 = 106
    assert_eq!(client.get_twap(), 106);
}

#[test]
fn test_update_too_soon() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.initialize(&token_a, &token_b, &60);
    client.update_price(&100);

    assert_eq!(
        e.as_contract(&contract_id, || TwapOracle::update_price(e.clone(), 110)),
        Err(Error::InsufficientTimeElapsed)
    );

    // Advance time by 50 seconds.
    e.ledger().with_mut(|li| li.timestamp = 1050);
    assert_eq!(
        e.as_contract(&contract_id, || TwapOracle::update_price(e.clone(), 110)),
        Err(Error::InsufficientTimeElapsed)
    );

    // Advance to 1060.
    // Advance to 1060
    e.ledger().with_mut(|li| li.timestamp = 1060);
    assert_eq!(
        e.as_contract(&contract_id, || TwapOracle::update_price(e.clone(), 110)),
        Ok(())
    );
}

#[test]
fn test_staleness_disabled_by_default() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.initialize(&token_a, &token_b, &60);
    client.update_price(&100);
    e.ledger().with_mut(|li| li.timestamp = 1100);
    client.update_price(&100);

    // Even after a long time, with checks disabled the feed stays fresh and
    // keeps reporting its stable TWAP (100).
    e.ledger().with_mut(|li| li.timestamp = 1000 + 24 * 60 * 60);
    assert_eq!(client.is_price_stale(), false);
    assert_eq!(client.latest_price(), 100);
}

#[test]
fn test_stale_detection_after_max_age() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.initialize(&token_a, &token_b, &60);
    client.update_price(&100);
    e.ledger().with_mut(|li| li.timestamp = 1100);
    client.update_price(&100);
    client.set_max_price_age(&300);

    // Fresh right at the boundary: 1100 + 300 = 1400 is not over the window.
    e.ledger().with_mut(|li| li.timestamp = 1400);
    assert_eq!(client.is_price_stale(), false);
    assert_eq!(client.latest_price(), 100);

    // Just over the window.
    e.ledger().with_mut(|li| li.timestamp = 1401);
    assert_eq!(client.is_price_stale(), true);
    assert_eq!(client.latest_price(), 0);

    // Updating the feed revives it; TWAP stays at 100.
    client.update_price(&110);
    assert_eq!(client.is_price_stale(), false);
    assert_eq!(client.latest_price(), 100);
}

#[test]
fn test_never_updated_feed_is_stale() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.initialize(&token_a, &token_b, &60);
    client.set_max_price_age(&300);

    assert_eq!(client.is_price_stale(), true);
    assert_eq!(client.latest_price(), 0);
    let zero = || TwapOracle::update_price(e.clone(), 0);
    let negative = || TwapOracle::update_price(e.clone(), -1);
    assert_eq!(e.as_contract(&contract_id, zero), Err(Error::InvalidPrice));
    assert_eq!(e.as_contract(&contract_id, negative), Err(Error::InvalidPrice));
}

#[test]
fn test_set_max_price_age_toggles_checks() {
    let e = Env::default();
    let (token_a, token_b) = setup_pair(&e);
    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.initialize(&token_a, &token_b, &60);
    client.update_price(&100);

    assert_eq!(client.set_max_price_age(&10), ());
    assert_eq!(client.get_max_price_age(), 10);

    e.ledger().with_mut(|li| li.timestamp = 1010);
    assert_eq!(client.is_price_stale(), false);

    e.ledger().with_mut(|li| li.timestamp = 1011);
    assert_eq!(client.is_price_stale(), true);

    // Disabling the check reports a fresh feed again.
    client.set_max_price_age(&0);
    assert_eq!(client.is_price_stale(), false);
    let update = || TwapOracle::update_price(e.clone(), 100);
    assert_eq!(e.as_contract(&contract_id, update), Err(Error::NotInitialized));
    assert_eq!(client.get_twap(), 0);
}

#[test]
fn test_state_getter_reflects_updates() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 5000);

    let contract_id = e.register(TwapOracle, ());
    let client = TwapOracleClient::new(&e, &contract_id);

    let token_a = Address::generate(&e);
    let token_b = Address::generate(&e);

    client.initialize(&token_a, &token_b, &10);
    client.update_price(&100);

    // Advance time and update again.
    e.ledger().with_mut(|li| li.timestamp = 5010);
    client.update_price(&110);

    let state = client.get_state();
    assert_eq!(state.last_price, 110);
    assert_eq!(state.last_update_timestamp, 5010);
    assert_eq!(state.cumulative_price, 100 * 10);
    assert_eq!(state.total_time, 10);
}