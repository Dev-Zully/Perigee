#![cfg(test)]

//! Metadata URI caching and validation tests for the token contract.

use crate::contract::{Token, TokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup(env: &Env) -> (TokenClient<'_>, Address) {
    let contract_id = env.register(Token, ());
    let client = TokenClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(
        &admin,
        &7,
        &String::from_str(env, "Test Token"),
        &String::from_str(env, "TEST"),
    );
    (client, admin)
}

/// A valid URI can be set and read back unchanged.
#[test]
fn test_set_and_read_token_uri() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    let uri = String::from_str(&env, "https://metadata.perigee.dev/tokens/1.json");
    client.set_token_uri(&uri);
    assert_eq!(client.token_uri(), uri);
}

/// URIs must use a supported scheme; others are rejected.
#[test]
#[should_panic]
fn test_invalid_scheme_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.set_token_uri(&String::from_str(&env, "file:///etc/passwd"));
}

/// Whitespace inside a URI is rejected.
#[test]
#[should_panic]
fn test_whitespace_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.set_token_uri(&String::from_str(
        &env,
        "https://metadata .perigee.dev/x.json",
    ));
}

/// URIs longer than the contract limit are rejected.
#[test]
#[should_panic]
fn test_oversized_uri_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    let uri = String::from_str(
        &env,
        "https://metadata.perigee.dev/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    client.set_token_uri(&uri);
}

/// A short TTL marks the cache stale after the ledger window passes.
#[test]
fn test_ttl_expiry_marks_cache_stale() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.set_token_uri_ttl(&10);
    client.set_token_uri(&String::from_str(
        &env,
        "https://metadata.perigee.dev/tokens/2.json",
    ));

    assert!(!client.token_uri_info().expired);

    env.ledger().with_mut(|l| l.sequence_number += 20);

    assert!(client.token_uri_info().expired);
}

/// Reading a stale cache freshens it without changing the URI.
#[test]
fn test_read_freshens_stale_cache() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.set_token_uri_ttl(&10);
    let uri = String::from_str(&env, "https://metadata.perigee.dev/tokens/3.json");
    client.set_token_uri(&uri);

    env.ledger().with_mut(|l| l.sequence_number += 20);
    assert!(client.token_uri_info().expired);

    assert_eq!(client.token_uri(), uri);
    assert!(!client.token_uri_info().expired);
}

/// Invalidation removes the cache; reads then return an empty URI.
#[test]
fn test_invalidate_token_uri() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.set_token_uri(&String::from_str(
        &env,
        "https://metadata.perigee.dev/tokens/4.json",
    ));
    client.invalidate_token_uri();

    assert_eq!(client.token_uri(), String::from_str(&env, ""));
    assert!(client.token_uri_info().expired);
}

/// TTL must be positive and within the allowed ceiling.
#[test]
#[should_panic]
fn test_zero_ttl_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.set_token_uri_ttl(&0);
}

/// URI writes require the admin's authorization.
#[test]
#[should_panic]
fn test_set_token_uri_requires_auth() {
    let env = Env::default();
    // No mock_all_auths — auth is enforced via admin.require_auth()
    let (client, _admin) = setup(&env);
    client.set_token_uri(&String::from_str(
        &env,
        "https://metadata.perigee.dev/x.json",
    ));
}

/// Metadata URI operations respect the METADATA pause.
#[test]
#[should_panic]
fn test_set_token_uri_respects_metadata_pause() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);
    client.guard_pause(&admin, &emergency_guard::PauseType::METADATA, &true);
    client.set_token_uri(&String::from_str(
        &env,
        "https://metadata.perigee.dev/x.json",
    ));
}
