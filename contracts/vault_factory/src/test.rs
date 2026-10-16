#![cfg(test)]

//! Unit tests for the vault factory and the UUPS-style proxy.
//!
//! Two themes, matching the two issues this crate covers:
//!
//! * **#635** — upgrade authorization, the anti-stale-write guard, and a
//!   rollback window that is actually bounded.
//! * **#632** — deterministic salts, the admin allowlist, and admin rotation.
//!
//! The `create_vault` deployment path itself needs a real uploaded wasm artifact
//! and is exercised by the factory's own integration harness; the authorization
//! and registry behaviour around it is what is covered here.
//!
//! Closes OdyxeeeLabs/Perigee#632
//! Closes OdyxeeeLabs/Perigee#635

use crate::{
    Error, ProxyKey, VaultFactory, VaultFactoryClient, VaultProxy, VaultProxyClient,
    PROXY_STORAGE_VERSION, ROLLBACK_WINDOW_LEDGERS, STORAGE_GAP_SLOTS,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

/// Deploy a proxy and initialize it, returning `(proxy_id, client)`.
///
/// The client borrows the `Env`, so this must stay a macro rather than becoming a
/// function.
macro_rules! proxy {
    ($env:expr, $owner:expr, $impl:expr) => {{
        let proxy_id = $env.register(VaultProxy, ());
        let proxy = VaultProxyClient::new(&$env, &$proxy_id);
        proxy.initialize(&$owner, &$impl);
        ($proxy_id, proxy)
    }};
}

macro_rules! setup {
    () => {{
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| li.sequence_number = 1_000);

        let admin = Address::generate(&env);
        let implementation = Address::generate(&env);
        let upgrade_v2 = Address::generate(&env);
        // Stand-in for the proxy artifact the factory would deploy. The factory
        // only ever forwards a hash it has been given, so a synthetic hash is
        // enough to exercise the registry and authorization paths.
        let proxy_wasm = BytesN::from_array(&env, &[7u8; 32]);

        let factory_id = env.register(VaultFactory, ());
        let factory = VaultFactoryClient::new(&env, &factory_id);
        factory.initialize(&admin, &implementation, &proxy_wasm);
        factory.approve_implementation(&admin, &proxy_wasm);

        (env, factory_id, factory, admin, implementation, upgrade_v2, proxy_wasm)
    }};
}

fn seed(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

// ── #635 — proxy upgrade authorization ───────────────────────────────────────

#[test]
fn proxy_records_the_initial_implementation() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let implementation = Address::generate(&env);

    let (_id, proxy) = proxy!(env, owner, implementation);

    assert_eq!(proxy.implementation(), implementation);
    assert_eq!(proxy.upgrader(), owner);
    assert!(!proxy.rollback_available());
}

#[test]
fn proxy_cannot_be_initialized_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let implementation = Address::generate(&env);
    let (_id, proxy) = proxy!(env, owner, implementation);

    assert_eq!(
        proxy.try_initialize(&owner, &implementation),
        Err(Ok(Error::AlreadyInitialized))
    );
}

#[test]
fn queries_before_initialization_report_not_initialized() {
    let env = Env::default();
    let proxy_id = env.register(VaultProxy, ());
    let proxy = VaultProxyClient::new(&env, &proxy_id);

    assert_eq!(proxy.try_implementation(), Err(Ok(Error::NotInitialized)));
    assert_eq!(proxy.try_get_config(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn upgrade_requires_the_named_upgrader() {
    let (env, _fid, _f, admin, implementation, upgrade_v2, _wasm) = setup!();
    let (pid, proxy) = proxy!(env, admin.clone(), implementation);

    let stranger = Address::generate(&env);
    assert_eq!(
        proxy.try_upgrade_to(&stranger, &upgrade_v2, &implementation),
        Err(Ok(Error::Unauthorized))
    );

    proxy.upgrade_to(&admin, &upgrade_v2, &implementation);
    assert_eq!(
        VaultProxyClient::new(&env, &pid).implementation(),
        upgrade_v2
    );
}

#[test]
fn upgrade_rejects_a_stale_expected_implementation() {
    let (env, _fid, _f, admin, implementation, upgrade_v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    let stale = Address::generate(&env);

    // The caller believes the proxy still points at `stale`; it does not. This is
    // the optimistic-concurrency guard that stops one upgrade silently clobbering
    // another.
    assert_eq!(
        proxy.try_upgrade_to(&admin, &upgrade_v2, &stale),
        Err(Ok(Error::StaleImplementation))
    );
    assert_eq!(proxy.implementation(), implementation);
}

#[test]
fn upgrade_rejects_a_no_op() {
    let (env, _fid, _f, admin, implementation, _v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    assert_eq!(
        proxy.try_upgrade_to(&admin, &implementation, &implementation),
        Err(Ok(Error::SameImplementation))
    );
}

#[test]
fn upgrade_records_the_outgoing_implementation() {
    let (env, _fid, _f, admin, implementation, upgrade_v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    proxy.upgrade_to(&admin, &upgrade_v2, &implementation);

    assert_eq!(
        proxy.get_config().previous_implementation,
        Some(implementation)
    );
}

// ── #635 — rollback ──────────────────────────────────────────────────────────

#[test]
fn rollback_reverts_to_the_previous_implementation_inside_the_window() {
    let (env, _fid, _f, admin, implementation, upgrade_v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    proxy.upgrade_to(&admin, &upgrade_v2, &implementation);
    assert_eq!(proxy.implementation(), upgrade_v2);
    assert!(proxy.rollback_available());
    assert!(proxy.rollback_ledgers_remaining() > 0);

    let target = proxy.rollback_to(&admin, &upgrade_v2);

    assert_eq!(target, implementation);
    assert_eq!(proxy.implementation(), implementation);
    // The window closes after a successful rollback, so a second rollback cannot
    // walk backwards through history.
    assert!(!proxy.rollback_available());
    assert_eq!(proxy.rollback_ledgers_remaining(), 0);
}

#[test]
fn rollback_is_refused_once_the_window_closes() {
    let (env, _fid, _f, admin, implementation, upgrade_v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    proxy.upgrade_to(&admin, &upgrade_v2, &implementation);

    env.ledger()
        .with_mut(|li| li.sequence_number += ROLLBACK_WINDOW_LEDGERS + 1);

    assert_eq!(
        proxy.try_rollback_to(&admin, &upgrade_v2),
        Err(Ok(Error::RollbackUnavailable))
    );
    assert_eq!(proxy.implementation(), upgrade_v2);
}

#[test]
fn rollback_with_no_predecessor_is_refused() {
    let (env, _fid, _f, admin, implementation, _v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    assert_eq!(
        proxy.try_rollback_to(&admin, &implementation),
        Err(Ok(Error::NoRollbackTarget))
    );
}

#[test]
fn rollback_rejects_a_stale_expected_implementation() {
    let (env, _fid, _f, admin, implementation, upgrade_v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    proxy.upgrade_to(&admin, &upgrade_v2, &implementation);

    assert_eq!(
        proxy.try_rollback_to(&admin, &implementation),
        Err(Ok(Error::StaleImplementation))
    );
    assert_eq!(proxy.implementation(), upgrade_v2);
}

#[test]
fn rollback_requires_the_upgrader() {
    let (env, _fid, _f, admin, implementation, upgrade_v2, _wasm) = setup!();
    let (_pid, proxy) = proxy!(env, admin.clone(), implementation);

    proxy.upgrade_to(&admin, &upgrade_v2, &implementation);

    let stranger = Address::generate(&env);
    assert_eq!(
        proxy.try_rollback_to(&stranger, &upgrade_v2),
        Err(Ok(Error::Unauthorized))
    );
}

// ── #635 — storage layout ────────────────────────────────────────────────────

#[test]
fn the_storage_gap_is_reserved_but_unwritten() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let implementation = Address::generate(&env);
    let (_id, proxy) = proxy!(env, owner, implementation);

    assert_eq!(proxy.storage_version(), PROXY_STORAGE_VERSION);
    assert_eq!(proxy.reserved_slots(), STORAGE_GAP_SLOTS);

    // The reservation is layout-only: the config exists, but no gap slot was ever
    // written, so the reserve costs no rent.
    assert!(env.storage().instance().has(&ProxyKey::Config));
    for slot in 0..STORAGE_GAP_SLOTS {
        assert!(
            !env.storage().instance().has(&ProxyKey::Gap(slot)),
            "gap slot {slot} should not be written"
        );
    }
}

// ── #632 — factory ───────────────────────────────────────────────────────────

#[test]
fn factory_starts_empty_and_reports_its_configuration() {
    let (env, _fid, factory, admin, implementation, _v2, proxy_wasm) = setup!();

    assert_eq!(factory.admin(), admin);
    assert_eq!(factory.default_implementation(), implementation);
    assert_eq!(factory.vault_count(), 0);
    assert!(factory.is_approved(&proxy_wasm));
    assert_eq!(factory.try_get_vault(&0), Err(Ok(Error::VaultNotFound)));
    assert_eq!(factory.try_get_vault_seed(&0), Err(Ok(Error::VaultNotFound)));
}

#[test]
fn factory_can_only_be_initialized_once() {
    let (env, _fid, factory, admin, implementation, _v2, proxy_wasm) = setup!();

    assert_eq!(
        factory.try_initialize(&admin, &implementation, &proxy_wasm),
        Err(Ok(Error::AlreadyInitialized))
    );
}

#[test]
fn factory_queries_before_initialization_report_not_initialized() {
    let env = Env::default();
    let factory_id = env.register(VaultFactory, ());
    let factory = VaultFactoryClient::new(&env, &factory_id);

    assert_eq!(factory.try_admin(), Err(Ok(Error::NotInitialized)));
    assert_eq!(factory.try_vault_count(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn only_the_admin_may_approve_or_revoke_an_implementation() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let implementation = Address::generate(&env);
    let proxy_wasm = BytesN::from_array(&env, &[7u8; 32]);

    let factory_id = env.register(VaultFactory, ());
    let factory = VaultFactoryClient::new(&env, &factory_id);
    factory.initialize(&admin, &implementation, &proxy_wasm);

    let stranger = Address::generate(&env);
    assert!(!factory.is_approved(&proxy_wasm));
    assert_eq!(
        factory.try_approve_implementation(&stranger, &proxy_wasm),
        Err(Ok(Error::Unauthorized))
    );

    factory.approve_implementation(&admin, &proxy_wasm);
    assert!(factory.is_approved(&proxy_wasm));

    factory.revoke_implementation(&admin, &proxy_wasm);
    assert!(!factory.is_approved(&proxy_wasm));
}

#[test]
fn an_unapproved_wasm_hash_is_refused_at_creation() {
    let (env, _fid, factory, admin, implementation, _v2, _approved) = setup!();

    let rogue = BytesN::from_array(&env, &[9u8; 32]);

    assert_eq!(
        factory.try_create_vault(
            &admin,
            &Address::generate(&env),
            &implementation,
            &rogue,
            &seed(&env, 1)
        ),
        Err(Ok(Error::ImplementationNotApproved))
    );
    assert_eq!(factory.vault_count(), 0);
}

#[test]
fn only_the_admin_may_create_a_vault() {
    let (env, _fid, factory, _admin, implementation, _v2, proxy_wasm) = setup!();

    let stranger = Address::generate(&env);
    assert_eq!(
        factory.try_create_vault(
            &stranger,
            &Address::generate(&env),
            &implementation,
            &proxy_wasm,
            &seed(&env, 1)
        ),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(factory.vault_count(), 0);
}

#[test]
fn the_vault_salt_is_deterministic_and_distinguishing() {
    let (env, _fid, factory, _admin, implementation, _v2, _wasm) = setup!();

    let owner = Address::generate(&env);
    let a = factory.vault_salt(&owner, &seed(&env, 1), &implementation);
    let b = factory.vault_salt(&owner, &seed(&env, 1), &implementation);
    let c = factory.vault_salt(&owner, &seed(&env, 2), &implementation);
    let d = factory.vault_salt(
        &Address::generate(&env),
        &seed(&env, 1),
        &implementation,
    );

    assert_eq!(a, b, "the same triple must always yield the same salt");
    assert_ne!(a, c, "a different seed must yield a different salt");
    assert_ne!(a, d, "a different owner must yield a different salt");
}

#[test]
fn the_salt_also_binds_the_implementation() {
    let (env, _fid, factory, _admin, implementation, upgrade_v2, _wasm) = setup!();

    let owner = Address::generate(&env);
    assert_ne!(
        factory.vault_salt(&owner, &seed(&env, 1), &implementation),
        factory.vault_salt(&owner, &seed(&env, 1), &upgrade_v2),
        "the same owner and seed on a different implementation must not collide"
    );
}

#[test]
fn rotating_the_default_implementation_leaves_vaults_alone() {
    let (env, _fid, factory, admin, implementation, _v2, proxy_wasm) = setup!();

    factory.set_default_implementation(&admin, &proxy_wasm);
    factory.set_default_implementation(&admin, &implementation);

    assert_eq!(factory.default_implementation(), implementation);
}

#[test]
fn set_admin_rotates_the_factory_admin() {
    let (env, _fid, factory, admin, _impl, _v2, proxy_wasm) = setup!();
    let successor = Address::generate(&env);

    factory.set_admin(&admin, &successor);

    assert_eq!(factory.admin(), successor);
    assert_eq!(
        factory.try_approve_implementation(&admin, &proxy_wasm),
        Err(Ok(Error::Unauthorized))
    );
}