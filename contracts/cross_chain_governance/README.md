# Perigee Cross-Chain Governance — message passing, remote execution, state sync

> Implements [OdyxeeeLabs/Perigee#650](https://github.com/OdyxeeeLabs/Perigee/issues/650)
> — *"Add cross-chain governance: message passing (LayerZero/Wormhole), remote execution, state sync"*
> (source: `contract.md` § Access Control & Governance, [#20](https://github.com/OdyxeeeLabs/Perigee/issues/20)).

## Overview

Cross-chain governance enables Perigee to coordinate governance decisions, execute proposals across multiple blockchain networks, and maintain synchronized governance state between the hub (Stellar/Soroban) and remote spoke chains.

Rather than relying on ad-hoc off-chain multisigs or fragmented governance silos, this contract formalizes:
1. **Multi-bridge message passing** (LayerZero, Wormhole, and Native protocol wire formats).
2. **Secure remote execution** with replay protection, sender authorization, and optional timelock delays.
3. **Bidirectional state synchronization** for proposals, voting snapshots, protocol parameters, and Merkle state roots.

## Architecture

```text
               ┌────────────────────────────────────────────────────────┐
               │              Cross-Chain Governance Hub                 │
               │               (perigee-cross-chain-gov)                │
               └──────────────────────────┬─────────────────────────────┘
                                          │
                  ┌───────────────────────┼────────────────────────┐
                  ▼                       ▼                        ▼
         ┌─────────────────┐     ┌─────────────────┐      ┌─────────────────┐
         │ Message Passing │     │ Remote Execute  │      │   State Sync    │
         │ (LZ / WH / Nat) │     │ (Immediate/Time)│      │(Props/Roots/Prm)│
         └─────────────────┘     └─────────────────┘      └─────────────────┘
```

## Features

### 1. Message Passing (LayerZero / Wormhole / Native)

The contract supports pluggable bridge wire protocols:

- **LayerZero (`LZ_GOV_V1`)**: Validates endpoint IDs (`src_eid`, `dst_eid`), nonce, and packet payload hash.
- **Wormhole (`WH_GOV_V1`)**: Validates emitter chain, guardian quorum constraints, sequence numbers, and payload hash.
- **Native (`NATIVE_GOV_V1`)**: Native domain-separated message hash with timestamp checks.

Anti-replay protection is strictly enforced by:
- Storing processed canonical message hashes in `ProcessedMessage(BytesN<32>)`.
- Tracking monotonic inbound sequence numbers per remote chain `InboundSequence(source_chain_id)`.
- Gating message ingestion through an authorized sender whitelist `TrustedSender(source_chain_id, sender)`.

### 2. Remote Execution

Authorized remote governance messages can execute privileged calls on local Soroban contracts:

- **Immediate Execution**: When `min_execution_delay == 0`, valid messages invoke target contracts directly via `env.invoke_contract(&target, &function, args)`.
- **Timelocked Queued Execution**: When `min_execution_delay > 0`, incoming actions transition to `Queued` status until the delay expires (`execute_after`). Anyone can then trigger `execute_queued(execution_id)` once ready.
- **Cancellation**: Admin can cancel a queued execution prior to execution.
- **Outbound Dispatch**: Admin/governance can dispatch remote execution payloads to spoke chains (`dispatch_remote_execution`), producing indexed relayer events and incrementing outbound sequence nonces.

### 3. State Sync

Governance state can be synchronized and queried cross-chain:

| State Item | Description | Functions |
|---|---|---|
| **Proposals** | Sync proposal outcome, for/against votes, and closed status | `sync_proposal_state`, `get_synced_proposal`, `broadcast_proposal_state` |
| **Voting Power** | Sync voter voting units and snapshot ID across chains | `sync_voting_power`, `get_synced_voting_power`, `broadcast_voting_power` |
| **Parameters** | Sync protocol-wide fee, risk, and configuration values | `sync_parameter`, `get_synced_parameter`, `broadcast_parameter` |
| **State Roots** | Sync Merkle state roots across epochs and heights | `sync_state_root`, `get_synced_state_root`, `get_latest_state_root`, `broadcast_state_root` |

## Emergency Controls

- **Circuit Breaker (`set_paused`)**: Instantly freeze cross-chain message processing during an active bridge compromise or exploit.
- **Admin Rotation (`transfer_admin`)**: Rotate administrative authority.
- **Protocol Toggles (`set_bridge_protocol`)**: Disable a compromised bridge wire format while keeping remaining bridges functional.
