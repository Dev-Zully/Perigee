//! Fix for #524 (CONTRACT-30): refactor cross-chain verifier to support
//! multiple bridge protocols via a `BridgeProtocolVerifier` trait.
//!
//! Production implementation lives in
//! `contracts/cross_chain_verifier/src/bridge.rs` with dispatch from
//! `verify_signed_message_with_protocol`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeProtocol {
    Native,
    LayerZero,
    Wormhole,
}

pub trait BridgeProtocolVerifier {
    fn protocol() -> BridgeProtocol;
    fn domain_separator() -> &'static [u8];
}

pub struct NativeBridge;
pub struct LayerZeroBridge;
pub struct WormholeBridge;

impl BridgeProtocolVerifier for NativeBridge {
    fn protocol() -> BridgeProtocol {
        BridgeProtocol::Native
    }
    fn domain_separator() -> &'static [u8] {
        b"CROSS_CHAIN_MESSAGE_V1"
    }
}

impl BridgeProtocolVerifier for LayerZeroBridge {
    fn protocol() -> BridgeProtocol {
        BridgeProtocol::LayerZero
    }
    fn domain_separator() -> &'static [u8] {
        b"LZ_PACKET_V1"
    }
}

impl BridgeProtocolVerifier for WormholeBridge {
    fn protocol() -> BridgeProtocol {
        BridgeProtocol::Wormhole
    }
    fn domain_separator() -> &'static [u8] {
        b"WH_VAA_V1"
    }
}

pub fn digest_prefix(protocol: BridgeProtocol) -> &'static [u8] {
    match protocol {
        BridgeProtocol::Native => NativeBridge::domain_separator(),
        BridgeProtocol::LayerZero => LayerZeroBridge::domain_separator(),
        BridgeProtocol::Wormhole => WormholeBridge::domain_separator(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_are_distinct() {
        assert_ne!(
            digest_prefix(BridgeProtocol::Native),
            digest_prefix(BridgeProtocol::LayerZero)
        );
        assert_ne!(
            digest_prefix(BridgeProtocol::LayerZero),
            digest_prefix(BridgeProtocol::Wormhole)
        );
    }

    #[test]
    fn trait_protocols_match() {
        assert_eq!(NativeBridge::protocol(), BridgeProtocol::Native);
        assert_eq!(LayerZeroBridge::protocol(), BridgeProtocol::LayerZero);
        assert_eq!(WormholeBridge::protocol(), BridgeProtocol::Wormhole);
    }
}
