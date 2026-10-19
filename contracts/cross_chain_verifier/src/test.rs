#![cfg(test)]

use crate::{
    layerzero_path_proof, wormhole_guardian_proof, BridgeProtocol, CrossChainMessage,
    CrossChainVerifier, CrossChainVerifierClient, SignatureAlgorithm, SignedMessage,
};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, Vec};

fn setup(env: &Env) -> (CrossChainVerifierClient, Address) {
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_initialization_enables_native_only() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let protocols = client.get_enabled_bridge_protocols();
    assert_eq!(protocols.len(), 1);
    assert_eq!(protocols.get(0).unwrap(), BridgeProtocol::Native);
}

#[test]
fn test_enable_layerzero_and_wormhole() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    client.enable_bridge_protocol(&BridgeProtocol::LayerZero);
    client.enable_bridge_protocol(&BridgeProtocol::Wormhole);
    // idempotent
    client.enable_bridge_protocol(&BridgeProtocol::LayerZero);

    let protocols = client.get_enabled_bridge_protocols();
    assert_eq!(protocols.len(), 3);
}

#[test]
fn test_layerzero_disabled_rejects_verification() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let msg = CrossChainMessage {
        source_chain: 1,
        destination_chain: 2,
        nonce: 7,
        payload: Bytes::from_slice(&env, b"hi"),
        timestamp: 100,
    };
    let signed = SignedMessage {
        message: msg.clone(),
        signature: BytesN::from_array(&env, &[0u8; 64]),
        signer_public_key: Bytes::from_slice(&env, &[1u8; 32]),
        algorithm: SignatureAlgorithm::Ed25519,
        revocation_nonce: 0,
    };
    let bridge_proof = layerzero_path_proof(&env, 1, 2);
    let proof: Vec<BytesN<32>> = Vec::new(&env);
    let flags: Vec<bool> = Vec::new(&env);

    // LayerZero not enabled yet → false
    let ok = client.verify_signed_message_with_protocol(
        &signed,
        &BridgeProtocol::LayerZero,
        &bridge_proof,
        &1u32,
        &proof,
        &flags,
    );
    assert!(!ok);
}

#[test]
fn test_root_update_and_merkle_verify() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let leaf = BytesN::from_array(&env, &[2; 32]);
    let sibling = BytesN::from_array(&env, &[3; 32]);

    let mut combined = [0u8; 64];
    combined[0..32].copy_from_slice(&sibling.to_array());
    combined[32..64].copy_from_slice(&leaf.to_array());
    let root_arr = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined))
        .to_array();
    let root = BytesN::from_array(&env, &root_arr);

    client.update_root(&10u32, &root);

    let mut proof = Vec::new(&env);
    proof.push_back(sibling);
    let mut flags = Vec::new(&env);
    flags.push_back(true);

    assert!(client.verify_message(&10u32, &leaf, &proof, &flags));
}

#[test]
fn test_wormhole_path_helpers_roundtrip() {
    let env = Env::default();
    let proof = wormhole_guardian_proof(&env, 13, 1);
    assert_eq!(proof.protocol, BridgeProtocol::Wormhole);
    assert_eq!(proof.proof_data.len(), 5);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_initialization() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
}
