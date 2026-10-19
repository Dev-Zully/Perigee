#![no_std]

mod bridge;

pub use bridge::{
    empty_native_proof, hash_for_protocol, layerzero_path_proof, validate_for_protocol,
    wormhole_guardian_proof, BridgeProof, BridgeProtocol, BridgeProtocolVerifier, LayerZeroBridge,
    NativeBridge, WormholeBridge,
};

use bridge::{hash_for_protocol, validate_for_protocol, verify_algorithm_signature, empty_native_proof};

use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignatureAlgorithm {
    Ed25519,
    Secp256k1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainMessage {
    pub source_chain: u32,
    pub destination_chain: u32,
    pub nonce: u64,
    pub payload: Bytes,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedMessage {
    pub message: CrossChainMessage,
    pub signature: BytesN<64>,
    pub signer_public_key: Bytes,
    pub algorithm: SignatureAlgorithm,
    pub revocation_nonce: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    StateRoot(u32),
    AuthorizedSigners,
    SignerAlgorithm(Bytes),
    ProcessedMessages(BytesN<32>),
    Nonces(Address),
    SignerCount,
    ProcessedNonce(u64),
    SignerRevocationNonce(Bytes),
    /// Enabled bridge protocols (defaults to Native-only when unset).
    EnabledProtocols,
}

#[contract]
pub struct CrossChainVerifier;

#[contractimpl]
impl CrossChainVerifier {
    /// Initialize the contract with an admin who has the right to update state roots.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::AuthorizedSigners, &Vec::new(&env));
        // Enable Native by default; LayerZero + Wormhole can be toggled by admin.
        let mut protocols: Vec<BridgeProtocol> = Vec::new(&env);
        protocols.push_back(BridgeProtocol::Native);
        env.storage()
            .persistent()
            .set(&DataKey::EnabledProtocols, &protocols);
    }

    /// Admin: enable an additional bridge protocol (LayerZero, Wormhole, …).
    pub fn enable_bridge_protocol(env: Env, protocol: BridgeProtocol) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        let mut protocols: Vec<BridgeProtocol> = env
            .storage()
            .persistent()
            .get(&DataKey::EnabledProtocols)
            .unwrap_or(Vec::new(&env));
        let mut i = 0;
        while i < protocols.len() {
            if protocols.get(i).unwrap() == protocol {
                return;
            }
            i += 1;
        }
        protocols.push_back(protocol.clone());
        env.storage()
            .persistent()
            .set(&DataKey::EnabledProtocols, &protocols);
        env.events()
            .publish(("bridge_protocol_enabled",), (protocol,));
    }

    /// Return currently enabled bridge protocols.
    pub fn get_enabled_bridge_protocols(env: Env) -> Vec<BridgeProtocol> {
        env.storage()
            .persistent()
            .get(&DataKey::EnabledProtocols)
            .unwrap_or({
                let mut v = Vec::new(&env);
                v.push_back(BridgeProtocol::Native);
                v
            })
    }

    fn is_protocol_enabled(env: &Env, protocol: &BridgeProtocol) -> bool {
        let protocols: Vec<BridgeProtocol> = env
            .storage()
            .persistent()
            .get(&DataKey::EnabledProtocols)
            .unwrap_or({
                let mut v = Vec::new(env);
                v.push_back(BridgeProtocol::Native);
                v
            });
        let mut i = 0;
        while i < protocols.len() {
            if &protocols.get(i).unwrap() == protocol {
                return true;
            }
            i += 1;
        }
        false
    }

    /// Update the state root for a specific block height.
    pub fn update_root(env: Env, block_height: u32, new_root: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::StateRoot(block_height), &new_root);
    }

    pub fn get_root(env: Env, block_height: u32) -> Option<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::StateRoot(block_height))
    }

    pub fn add_authorized_signer(env: Env, public_key: Bytes, algorithm: SignatureAlgorithm) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if env
            .storage()
            .persistent()
            .has(&DataKey::SignerAlgorithm(public_key.clone()))
        {
            panic!("Signer already authorized");
        }

        env.storage()
            .persistent()
            .set(&DataKey::SignerAlgorithm(public_key.clone()), &algorithm);
        env.storage()
            .persistent()
            .set(&DataKey::SignerRevocationNonce(public_key.clone()), &0u64);

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::SignerCount, &(count + 1));

        env.events().publish(("signer_added",), ());
    }

    pub fn remove_authorized_signer(env: Env, public_key: Bytes) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if !env
            .storage()
            .persistent()
            .has(&DataKey::SignerAlgorithm(public_key.clone()))
        {
            panic!("Signer not found");
        }

        env.storage()
            .persistent()
            .remove(&DataKey::SignerAlgorithm(public_key.clone()));

        let current_nonce: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::SignerRevocationNonce(public_key.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(
                &DataKey::SignerRevocationNonce(public_key),
                &(current_nonce + 1),
            );

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        if count > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::SignerCount, &(count - 1));
        }

        env.events().publish(("signer_removed",), ());
    }

    pub fn get_authorized_signers(env: Env) -> Vec<(Bytes, SignatureAlgorithm)> {
        Vec::new(&env)
    }

    pub fn get_signer_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::SignerCount)
            .unwrap_or(0)
    }

    pub fn has_authorized_signer(env: Env, public_key: Bytes) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::SignerAlgorithm(public_key))
    }

    /// Legacy entrypoint — verifies using the Native bridge protocol.
    pub fn verify_signed_message(
        env: Env,
        signed_message: SignedMessage,
        block_height: u32,
        proof: Vec<BytesN<32>>,
        proof_flags: Vec<bool>,
    ) -> bool {
        let bridge_proof = empty_native_proof(&env);
        Self::verify_signed_message_with_protocol(
            env,
            signed_message,
            BridgeProtocol::Native,
            bridge_proof,
            block_height,
            proof,
            proof_flags,
        )
    }

    /// Multi-bridge entrypoint: abstract signature verification via [`BridgeProtocol`].
    ///
    /// Adding LayerZero / Wormhole (or future bridges) only requires a new
    /// [`bridge::BridgeProtocolVerifier`] impl and a match arm in the bridge module.
    pub fn verify_signed_message_with_protocol(
        env: Env,
        signed_message: SignedMessage,
        protocol: BridgeProtocol,
        bridge_proof: BridgeProof,
        block_height: u32,
        proof: Vec<BytesN<32>>,
        proof_flags: Vec<bool>,
    ) -> bool {
        if !Self::is_protocol_enabled(&env, &protocol) {
            return false;
        }

        if !validate_for_protocol(&env, &protocol, &signed_message.message, &bridge_proof) {
            return false;
        }

        if !Self::verify_signature_for_protocol(&env, &signed_message, &protocol, &bridge_proof) {
            return false;
        }

        let message_hash =
            hash_for_protocol(&env, &protocol, &signed_message.message, &bridge_proof);
        if env
            .storage()
            .persistent()
            .has(&DataKey::ProcessedMessages(message_hash.clone()))
        {
            return false;
        }

        if !Self::verify_merkle_proof(&env, &message_hash, &block_height, &proof, &proof_flags) {
            return false;
        }

        env.storage()
            .persistent()
            .set(&DataKey::ProcessedMessages(message_hash), &true);

        env.events().publish(
            ("message_verified",),
            (
                protocol,
                signed_message.message.source_chain,
                signed_message.message.destination_chain,
                signed_message.message.nonce,
            ),
        );

        true
    }

    pub fn verify_message(
        env: Env,
        block_height: u32,
        leaf: BytesN<32>,
        proof: Vec<BytesN<32>>,
        proof_flags: Vec<bool>,
    ) -> bool {
        Self::verify_merkle_proof(&env, &leaf, &block_height, &proof, &proof_flags)
    }

    pub fn verify_message_and_consume(
        env: Env,
        block_height: u32,
        nonce: u64,
        leaf: BytesN<32>,
        proof: Vec<BytesN<32>>,
        proof_flags: Vec<bool>,
    ) -> bool {
        if Self::is_nonce_processed(env.clone(), nonce) {
            panic!("nonce already processed");
        }

        let valid = Self::verify_message(env.clone(), block_height, leaf, proof, proof_flags);
        if !valid {
            return false;
        }

        Self::consume_nonce(&env, nonce);
        true
    }

    pub fn is_nonce_processed(env: Env, nonce: u64) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::ProcessedNonce(nonce))
            .unwrap_or(false)
    }
}

impl CrossChainVerifier {
    fn verify_signature_for_protocol(
        env: &Env,
        signed_message: &SignedMessage,
        protocol: &BridgeProtocol,
        bridge_proof: &BridgeProof,
    ) -> bool {
        let signer_algorithm: Option<SignatureAlgorithm> = env.storage().persistent().get(
            &DataKey::SignerAlgorithm(signed_message.signer_public_key.clone()),
        );

        let signer_algorithm = match signer_algorithm {
            Some(algo) => algo,
            None => return false,
        };

        let current_nonce: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::SignerRevocationNonce(
                signed_message.signer_public_key.clone(),
            ))
            .unwrap_or(0);

        if signed_message.revocation_nonce < current_nonce {
            return false;
        }

        let message_hash =
            hash_for_protocol(env, protocol, &signed_message.message, bridge_proof);

        verify_algorithm_signature(
            env,
            &signer_algorithm,
            &message_hash,
            &signed_message.signature,
            &signed_message.signer_public_key,
        )
    }

    fn verify_merkle_proof(
        env: &Env,
        leaf: &BytesN<32>,
        block_height: &u32,
        proof: &Vec<BytesN<32>>,
        proof_flags: &Vec<bool>,
    ) -> bool {
        let expected_root: BytesN<32> = match env
            .storage()
            .persistent()
            .get(&DataKey::StateRoot(*block_height))
        {
            Some(root) => root,
            None => return false,
        };

        if proof.len() != proof_flags.len() {
            return false;
        }

        let mut current_hash = leaf.to_array();

        let mut i = 0;
        while i < proof.len() {
            let sibling = proof.get(i).unwrap().to_array();
            let is_left_sibling = proof_flags.get(i).unwrap();

            let mut combined = [0u8; 64];
            if is_left_sibling {
                combined[0..32].copy_from_slice(&sibling);
                combined[32..64].copy_from_slice(&current_hash);
            } else {
                combined[0..32].copy_from_slice(&current_hash);
                combined[32..64].copy_from_slice(&sibling);
            }

            let combined_bytes = Bytes::from_slice(env, &combined);
            current_hash = env.crypto().sha256(&combined_bytes).to_array();
            i += 1;
        }

        let computed_root = BytesN::from_array(env, &current_hash);
        computed_root == expected_root
    }

    fn consume_nonce(env: &Env, nonce: u64) {
        env.storage()
            .persistent()
            .set(&DataKey::ProcessedNonce(nonce), &true);
    }
}

mod test;
