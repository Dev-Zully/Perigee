extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol, TryIntoVal, Val, Vec};

#[test]
fn test_proxy_upgrade_keeps_state_and_changes_logic() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);

    proxy.initialize(&admin, &impl_v1_id);
    assert_eq!(proxy.get_admin(), admin);
    assert_eq!(proxy.get_implementation(), impl_v1_id);
    assert_eq!(proxy.get_value(), 0);

    let first = proxy.increment(&7);
    assert_eq!(first, 7);
    assert_eq!(proxy.get_value(), 7);

    // Target implementation must be allowlisted before an upgrade is allowed.
    proxy.register_implementation(&impl_v2_id);
    assert_eq!(proxy.get_verified_implementations().len(), 2);

    proxy.upgrade_to(&impl_v2_id);
    assert_eq!(proxy.get_implementation(), impl_v2_id);

    let second = proxy.increment(&3);
    assert_eq!(second, 20);
    assert_eq!(proxy.get_value(), 20);

    let version_symbol = Symbol::new(&env, "version");
    let version_val: Val = proxy.delegate_call(&version_symbol, &Vec::new(&env));
    let version: u32 = version_val.try_into_val(&env).unwrap();
    assert_eq!(version, 2);
}

#[test]
fn test_upgrade_to_and_call_executes_new_implementation_method() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);
    proxy.register_implementation(&impl_v2_id);

    // Upgrade to the registered v2 implementation and immediately call through it.
    let call_result: Val =
        proxy.upgrade_to_and_call(&impl_v2_id, &Symbol::new(&env, "version"), &Vec::new(&env));
    let version: u32 = call_result.try_into_val(&env).unwrap();
    assert_eq!(version, 2);
    assert_eq!(proxy.get_implementation(), impl_v2_id);
}

#[test]
fn test_upgrade_rejects_unverified_implementation() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);
    assert_eq!(proxy.get_implementation(), impl_v1_id);

    // The implementation was never registered, so the upgrade must be refused.
    assert_eq!(
        env.as_contract(&proxy_id, || Proxy::upgrade_to(env.clone(), impl_v2_id.clone())),
        Err(ProxyError::UnverifiedImplementation)
    );
    assert_eq!(proxy.get_implementation(), impl_v1_id);

    // Registering it makes the upgrade succeed.
    proxy.register_implementation(&impl_v2_id);
    assert_eq!(proxy.upgrade_to(&impl_v2_id), ());
    assert_eq!(proxy.get_implementation(), impl_v2_id);
}

#[test]
fn test_self_upgrade_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    // Re-pointing at the already-active implementation is a no-op and must fail.
    assert_eq!(
        env.as_contract(&proxy_id, || Proxy::upgrade_to(env.clone(), impl_v1_id.clone())),
        Err(ProxyError::SelfUpgrade)
    );
}

#[test]
fn test_upgrade_and_call_shares_the_same_guards() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    // Unregistered implementation: upgrade_and_call must refuse and keep state.
    let method = Symbol::new(&env, "version");
    assert!(
        matches!(
            env.as_contract(
                &proxy_id,
                || Proxy::upgrade_to_and_call(env.clone(), impl_v2_id.clone(), method.clone(), Vec::new(&env))
            ),
            Err(ProxyError::UnverifiedImplementation)
        )
    );
    assert_eq!(proxy.get_implementation(), impl_v1_id);

    proxy.register_implementation(&impl_v2_id);
    let version_val: Val = proxy.upgrade_to_and_call(&impl_v2_id, &method, &Vec::new(&env));
    let version: u32 = version_val.try_into_val(&env).unwrap();
    assert_eq!(version, 2);
    assert_eq!(proxy.get_implementation(), impl_v2_id);
}

#[test]
fn test_unregister_blocks_future_upgrades() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);
    proxy.register_implementation(&impl_v2_id);

    proxy.unregister_implementation(&impl_v2_id);
    assert_eq!(proxy.get_verified_implementations().len(), 1);

    // The allowlisted address is gone, so the upgrade is refused again.
    assert_eq!(
        env.as_contract(&proxy_id, || Proxy::upgrade_to(env.clone(), impl_v2_id.clone())),
        Err(ProxyError::UnverifiedImplementation)
    );
}

#[test]
fn test_register_and_unregister_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    // Impl v1 is registered during initialization.
    assert_eq!(
        env.as_contract(&proxy_id, || Proxy::register_implementation(env.clone(), impl_v1_id.clone())),
        Err(ProxyError::AlreadyVerified)
    );
    // Unregistering something never registered fails.
    assert_eq!(
        env.as_contract(&proxy_id, || Proxy::unregister_implementation(env.clone(), impl_v2_id.clone())),
        Err(ProxyError::NotVerified)
    );
}
