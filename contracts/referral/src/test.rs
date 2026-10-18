#![cfg(test)]

use crate::{Error, Referral, ReferralClient, ReferralConfig};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(Referral, ());
    let client = ReferralClient::new(&env, &id);
    client.initialize(
        &admin,
        &ReferralConfig {
            reward_bps: 100, // 1%
            max_referees: 3,
            min_referee_age_ledgers: 5,
            min_volume_for_reward: 100,
        },
    );
    (env, id, admin)
}

#[test]
fn bind_and_lookup_referrer() {
    let (env, id, _) = setup();
    let client = ReferralClient::new(&env, &id);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);
    client.bind_referrer(&referee, &referrer);
    assert_eq!(client.get_referrer(&referee), Some(referrer.clone()));
    let stats = client.get_stats(&referrer);
    assert_eq!(stats.referee_count, 1);
}

#[test]
fn rejects_self_referral_and_double_bind() {
    let (env, id, _) = setup();
    let client = ReferralClient::new(&env, &id);
    let user = Address::generate(&env);
    assert_eq!(
        client.try_bind_referrer(&user, &user),
        Err(Ok(Error::SelfReferral))
    );
    let referrer = Address::generate(&env);
    client.bind_referrer(&user, &referrer);
    assert_eq!(
        client.try_bind_referrer(&user, &Address::generate(&env)),
        Err(Ok(Error::AlreadyReferred))
    );
}

#[test]
fn enforces_referee_cap() {
    let (env, id, _) = setup();
    let client = ReferralClient::new(&env, &id);
    let referrer = Address::generate(&env);
    for _ in 0..3 {
        client.bind_referrer(&Address::generate(&env), &referrer);
    }
    assert_eq!(
        client.try_bind_referrer(&Address::generate(&env), &referrer),
        Err(Ok(Error::RefereeLimit))
    );
}

#[test]
fn volume_requires_age_and_minimum() {
    let (env, id, admin) = setup();
    let client = ReferralClient::new(&env, &id);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);
    client.bind_referrer(&referee, &referrer);

    assert_eq!(
        client.try_attribute_volume(&referee, &50),
        Err(Ok(Error::VolumeTooLow))
    );
    assert_eq!(
        client.try_attribute_volume(&referee, &1_000),
        Err(Ok(Error::RefereeTooNew))
    );

    env.ledger().with_mut(|l| l.sequence_number += 10);
    let reward = client.attribute_volume(&referee, &1_000);
    assert_eq!(reward, 10); // 1% of 1000

    client.fund_pool(&admin, &100);
    let claimed = client.claim_rewards(&referrer);
    assert_eq!(claimed, 10);
    assert_eq!(client.reward_pool(), 90);
}
