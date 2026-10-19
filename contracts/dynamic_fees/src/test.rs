#![cfg(test)]

use crate::{
    compute_dynamic_fee, DynamicFeeConfig, DynamicFees, DynamicFeesClient, Error, FeeSignals,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn default_config() -> DynamicFeeConfig {
    DynamicFeeConfig {
        base_fee_bps: 30,
        min_fee_bps: 5,
        max_fee_bps: 100,
        utilization_weight_bps: 40,
        volatility_weight_bps: 20,
        max_undercut_bps: 10,
        max_premium_bps: 15,
    }
}

#[test]
fn utilization_raises_fee() {
    let config = default_config();
    let low = FeeSignals {
        utilization_bps: 1_000,
        volatility_bps: 0,
        competitor_fee_bps: 0,
        competitor_enabled: false,
    };
    let high = FeeSignals {
        utilization_bps: 9_000,
        volatility_bps: 0,
        competitor_fee_bps: 0,
        competitor_enabled: false,
    };
    let q_low = compute_dynamic_fee(&config, &low).unwrap();
    let q_high = compute_dynamic_fee(&config, &high).unwrap();
    assert!(q_high.fee_bps > q_low.fee_bps);
}

#[test]
fn volatility_raises_fee() {
    let config = default_config();
    let calm = FeeSignals {
        utilization_bps: 0,
        volatility_bps: 500,
        competitor_fee_bps: 0,
        competitor_enabled: false,
    };
    let wild = FeeSignals {
        utilization_bps: 0,
        volatility_bps: 5_000,
        competitor_fee_bps: 0,
        competitor_enabled: false,
    };
    assert!(
        compute_dynamic_fee(&config, &wild).unwrap().fee_bps
            > compute_dynamic_fee(&config, &calm).unwrap().fee_bps
    );
}

#[test]
fn competitor_clamp_prevents_undercutting() {
    let config = default_config();
    // Raw fee would be base 30; competitor at 50 with max undercut 10 → floor 40
    let signals = FeeSignals {
        utilization_bps: 0,
        volatility_bps: 0,
        competitor_fee_bps: 50,
        competitor_enabled: true,
    };
    let q = compute_dynamic_fee(&config, &signals).unwrap();
    assert!(q.fee_bps >= 40);
}

#[test]
fn fee_clamped_to_max() {
    let mut config = default_config();
    config.max_fee_bps = 35;
    let signals = FeeSignals {
        utilization_bps: 10_000,
        volatility_bps: 10_000,
        competitor_fee_bps: 0,
        competitor_enabled: false,
    };
    let q = compute_dynamic_fee(&config, &signals).unwrap();
    assert_eq!(q.fee_bps, 35);
}

#[test]
fn contract_quote_matches_pure_function() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(DynamicFees, ());
    let client = DynamicFeesClient::new(&env, &id);
    let config = default_config();
    client.initialize(&admin, &config);

    let signals = FeeSignals {
        utilization_bps: 5_000,
        volatility_bps: 2_000,
        competitor_fee_bps: 0,
        competitor_enabled: false,
    };
    assert_eq!(
        client.quote(&signals),
        compute_dynamic_fee(&config, &signals).unwrap()
    );
}

#[test]
fn invalid_utilization_rejected() {
    let config = default_config();
    let signals = FeeSignals {
        utilization_bps: 10_001,
        volatility_bps: 0,
        competitor_fee_bps: 0,
        competitor_enabled: false,
    };
    assert_eq!(
        compute_dynamic_fee(&config, &signals),
        Err(Error::InvalidInput)
    );
}
