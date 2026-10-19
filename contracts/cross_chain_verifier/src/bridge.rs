//! Multi-bridge protocol abstraction for cross-chain verification.
//!
//! Signature verification is abstracted behind [`BridgeProtocolVerifier`] so new
//! bridge formats (LayerZero, Wormhole, …) can be added without changing the
//! core [`crate::CrossChainVerifier`] pipeline.

use soroban_sdk::{contracttype, Bytes, BytesN, Env};

use crate::{CrossChainMessage, SignatureAlgorithm};

/// Supported bridge wire formats.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BridgeProtocol {
    /// Native Perigee format: authorized signer + Merkle inclusion proof.
    Native,
    /// LayerZero-style packet: src/dst endpoint ids + nonce + GUID domain sep.
    LayerZero,
    /// Wormhole-style VAA: emitter chain/address + sequence + guardian quorum.
    Wormhole,
}

/// Protocol-specific proof payload attached to a verification request.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeProof {
    pub protocol: BridgeProtocol,
    /// Opaque protocol bytes (packet header, VAA body fragment, etc.).
    pub proof_data: Bytes,
}

/// Trait that each bridge adapter implements.
///
/// Contract code dispatches on [`BridgeProtocol`] and calls the matching
/// adapter so adding a protocol is a localized change.
pub trait BridgeProtocolVerifier {
    fn protocol() -> BridgeProtocol;

    /// Domain-separated digest used for signature checks.
    fn hash_message(env: &Env, message: &CrossChainMessage, proof: &BridgeProof) -> BytesN<32>;

    /// Validate protocol-specific structural constraints on `proof`.
    fn validate_proof(env: &Env, message: &CrossChainMessage, proof: &BridgeProof) -> bool;
}

/// Native adapter — preserves CROSS_CHAIN_MESSAGE_V1 domain separation.
pub struct NativeBridge;

impl BridgeProtocolVerifier for NativeBridge {
    fn protocol() -> BridgeProtocol {
        BridgeProtocol::Native
    }

    fn hash_message(env: &Env, message: &CrossChainMessage, _proof: &BridgeProof) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.append(&Bytes::from_slice(env, b"CROSS_CHAIN_MESSAGE_V1"));
        data.append(&Bytes::from_slice(env, &message.source_chain.to_be_bytes()));
        data.append(&Bytes::from_slice(
            env,
            &message.destination_chain.to_be_bytes(),
        ));
        data.append(&Bytes::from_slice(env, &message.nonce.to_be_bytes()));
        data.append(&Bytes::from_slice(env, &message.timestamp.to_be_bytes()));
        let payload_hash = env.crypto().sha256(&message.payload);
        data.append(&payload_hash);
        env.crypto().sha256(&data).into()
    }

    fn validate_proof(_env: &Env, _message: &CrossChainMessage, proof: &BridgeProof) -> bool {
        proof.protocol == BridgeProtocol::Native
    }
}

/// LayerZero adapter — GUID-style domain: "LZ_PACKET_V1" || src || dst || nonce || payload.
pub struct LayerZeroBridge;

impl BridgeProtocolVerifier for LayerZeroBridge {
    fn protocol() -> BridgeProtocol {
        BridgeProtocol::LayerZero
    }

    fn hash_message(env: &Env, message: &CrossChainMessage, proof: &BridgeProof) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.append(&Bytes::from_slice(env, b"LZ_PACKET_V1"));
        data.append(&Bytes::from_slice(env, &message.source_chain.to_be_bytes()));
        data.append(&Bytes::from_slice(
            env,
            &message.destination_chain.to_be_bytes(),
        ));
        data.append(&Bytes::from_slice(env, &message.nonce.to_be_bytes()));
        if proof.proof_data.len() > 0 {
            data.append(&proof.proof_data);
        }
        let payload_hash = env.crypto().sha256(&message.payload);
        data.append(&payload_hash);
        env.crypto().sha256(&data).into()
    }

    fn validate_proof(_env: &Env, message: &CrossChainMessage, proof: &BridgeProof) -> bool {
        if proof.protocol != BridgeProtocol::LayerZero {
            return false;
        }
        if proof.proof_data.len() >= 8 {
            let bytes = proof.proof_data.clone();
            let mut src_arr = [0u8; 4];
            let mut dst_arr = [0u8; 4];
            for i in 0..4 {
                src_arr[i as usize] = bytes.get(i).unwrap_or(0);
                dst_arr[i as usize] = bytes.get(i + 4).unwrap_or(0);
            }
            let src = u32::from_be_bytes(src_arr);
            let dst = u32::from_be_bytes(dst_arr);
            if src != message.source_chain || dst != message.destination_chain {
                return false;
            }
        }
        true
    }
}

/// Wormhole adapter — VAA-style domain: "WH_VAA_V1" || emitter_chain || sequence || payload.
pub struct WormholeBridge;

impl BridgeProtocolVerifier for WormholeBridge {
    fn protocol() -> BridgeProtocol {
        BridgeProtocol::Wormhole
    }

    fn hash_message(env: &Env, message: &CrossChainMessage, proof: &BridgeProof) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.append(&Bytes::from_slice(env, b"WH_VAA_V1"));
        data.append(&Bytes::from_slice(env, &message.source_chain.to_be_bytes()));
        data.append(&Bytes::from_slice(env, &message.nonce.to_be_bytes()));
        if proof.proof_data.len() > 0 {
            data.append(&proof.proof_data);
        }
        let payload_hash = env.crypto().sha256(&message.payload);
        data.append(&payload_hash);
        env.crypto().sha256(&data).into()
    }

    fn validate_proof(_env: &Env, message: &CrossChainMessage, proof: &BridgeProof) -> bool {
        if proof.protocol != BridgeProtocol::Wormhole {
            return false;
        }
        if proof.proof_data.len() >= 1 {
            let quorum = proof.proof_data.get(0).unwrap_or(0);
            if quorum == 0 || quorum > 19 {
                return false;
            }
        }
        if proof.proof_data.len() >= 5 {
            let mut chain_arr = [0u8; 4];
            for i in 0..4 {
                chain_arr[i as usize] = proof.proof_data.get(i + 1).unwrap_or(0);
            }
            let emitter_chain = u32::from_be_bytes(chain_arr);
            if emitter_chain != message.source_chain {
                return false;
            }
        }
        true
    }
}

/// Dispatch helpers used by the contract — new protocols register here only.
pub fn hash_for_protocol(
    env: &Env,
    protocol: &BridgeProtocol,
    message: &CrossChainMessage,
    proof: &BridgeProof,
) -> BytesN<32> {
    match protocol {
        BridgeProtocol::Native => NativeBridge::hash_message(env, message, proof),
        BridgeProtocol::LayerZero => LayerZeroBridge::hash_message(env, message, proof),
        BridgeProtocol::Wormhole => WormholeBridge::hash_message(env, message, proof),
    }
}

pub fn validate_for_protocol(
    env: &Env,
    protocol: &BridgeProtocol,
    message: &CrossChainMessage,
    proof: &BridgeProof,
) -> bool {
    if &proof.protocol != protocol {
        return false;
    }
    match protocol {
        BridgeProtocol::Native => NativeBridge::validate_proof(env, message, proof),
        BridgeProtocol::LayerZero => LayerZeroBridge::validate_proof(env, message, proof),
        BridgeProtocol::Wormhole => WormholeBridge::validate_proof(env, message, proof),
    }
}

/// Empty native proof helper for callers that only use the legacy path.
pub fn empty_native_proof(env: &Env) -> BridgeProof {
    BridgeProof {
        protocol: BridgeProtocol::Native,
        proof_data: Bytes::new(env),
    }
}

/// Verify signature bytes against a protocol-specific message digest.
pub fn verify_algorithm_signature(
    env: &Env,
    algorithm: &SignatureAlgorithm,
    message_hash: &BytesN<32>,
    signature: &BytesN<64>,
    public_key: &Bytes,
) -> bool {
    // Match the host crypto calling convention used by the legacy verifier path.
    match algorithm {
        SignatureAlgorithm::Ed25519 => {
            if public_key.len() != 32 {
                return false;
            }
            env.crypto().ed25519_verify(
                public_key,
                &message_hash.to_bytes(),
                &signature.to_bytes(),
            )
        }
        SignatureAlgorithm::Secp256k1 => {
            if public_key.len() != 33 && public_key.len() != 65 {
                return false;
            }
            env.crypto().secp256k1_verify(
                public_key,
                &message_hash.to_bytes(),
                &signature.to_bytes(),
            )
        }
    }
}

/// Convenience: build LayerZero path id proof_data (src_eid || dst_eid).
pub fn layerzero_path_proof(env: &Env, src_eid: u32, dst_eid: u32) -> BridgeProof {
    let mut data = Bytes::new(env);
    data.append(&Bytes::from_slice(env, &src_eid.to_be_bytes()));
    data.append(&Bytes::from_slice(env, &dst_eid.to_be_bytes()));
    BridgeProof {
        protocol: BridgeProtocol::LayerZero,
        proof_data: data,
    }
}

/// Convenience: build Wormhole proof_data (quorum || emitter_chain).
pub fn wormhole_guardian_proof(env: &Env, quorum: u8, emitter_chain: u32) -> BridgeProof {
    let mut data = Bytes::new(env);
    data.append(&Bytes::from_slice(env, &[quorum]));
    data.append(&Bytes::from_slice(env, &emitter_chain.to_be_bytes()));
    BridgeProof {
        protocol: BridgeProtocol::Wormhole,
        proof_data: data,
    }
}

#[cfg(test)]
mod bridge_unit_tests {
    use super::*;
    use soroban_sdk::Env;

    fn sample_message(env: &Env) -> CrossChainMessage {
        CrossChainMessage {
            source_chain: 1,
            destination_chain: 2,
            nonce: 42,
            payload: Bytes::from_slice(env, b"payload"),
            timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn native_and_lz_digests_differ() {
        let env = Env::default();
        let msg = sample_message(&env);
        let native = empty_native_proof(&env);
        let lz = layerzero_path_proof(&env, 1, 2);
        let h_native = hash_for_protocol(&env, &BridgeProtocol::Native, &msg, &native);
        let h_lz = hash_for_protocol(&env, &BridgeProtocol::LayerZero, &msg, &lz);
        assert_ne!(h_native, h_lz);
    }

    #[test]
    fn wormhole_rejects_zero_quorum() {
        let env = Env::default();
        let msg = sample_message(&env);
        let bad = wormhole_guardian_proof(&env, 0, 1);
        assert!(!validate_for_protocol(
            &env,
            &BridgeProtocol::Wormhole,
            &msg,
            &bad
        ));
        let good = wormhole_guardian_proof(&env, 13, 1);
        assert!(validate_for_protocol(
            &env,
            &BridgeProtocol::Wormhole,
            &msg,
            &good
        ));
    }

    #[test]
    fn layerzero_rejects_mismatched_path() {
        let env = Env::default();
        let msg = sample_message(&env);
        let bad = layerzero_path_proof(&env, 9, 9);
        assert!(!validate_for_protocol(
            &env,
            &BridgeProtocol::LayerZero,
            &msg,
            &bad
        ));
        let good = layerzero_path_proof(&env, 1, 2);
        assert!(validate_for_protocol(
            &env,
            &BridgeProtocol::LayerZero,
            &msg,
            &good
        ));
    }

    #[test]
    fn protocol_mismatch_fails_validation() {
        let env = Env::default();
        let msg = sample_message(&env);
        let lz = layerzero_path_proof(&env, 1, 2);
        assert!(!validate_for_protocol(
            &env,
            &BridgeProtocol::Wormhole,
            &msg,
            &lz
        ));
    }
}
