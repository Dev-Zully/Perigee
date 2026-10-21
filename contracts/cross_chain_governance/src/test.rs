use super::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    vec, IntoVal,
};

#[contract]
pub struct MockTarget;

#[contractimpl]
impl MockTarget {
    pub fn upd_val(env: Env, val: u32) -> u32 {
        env.storage().instance().set(&symbol_short!("val"), &val);
        val
    }

    pub fn get_val(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("val"))
            .unwrap_or(0)
    }
}

fn setup_test(
    env: &Env,
    local_chain_id: u32,
    min_delay: u64,
) -> (
    Address,
    CrossChainGovernanceClient<'_>,
    Address,
    MockTargetClient<'_>,
) {
    let admin = Address::generate(env);
    let contract_id = env.register(CrossChainGovernance, ());
    let client = CrossChainGovernanceClient::new(env, &contract_id);
    client.initialize(&admin, &local_chain_id, &min_delay);

    let target_id = env.register(MockTarget, ());
    let target_client = MockTargetClient::new(env, &target_id);

    (admin, client, target_id, target_client)
}

#[test]
fn test_initialization_and_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, _, _) = setup_test(&env, 1, 0);

    let cfg = client.get_config();
    assert_eq!(cfg.admin, admin);
    assert_eq!(cfg.local_chain_id, 1);
    assert_eq!(cfg.min_execution_delay, 0);
    assert!(!cfg.paused);

    assert!(client.is_bridge_protocol_enabled(&BridgeProtocol::Native));
    assert!(client.is_bridge_protocol_enabled(&BridgeProtocol::LayerZero));
    assert!(client.is_bridge_protocol_enabled(&BridgeProtocol::Wormhole));
}

#[test]
fn test_admin_operations() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, _, _) = setup_test(&env, 1, 0);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    assert_eq!(client.get_config().admin, new_admin);

    client.set_execution_delay(&3600);
    assert_eq!(client.get_config().min_execution_delay, 3600);

    client.set_paused(&true);
    assert!(client.get_config().paused);

    client.set_bridge_protocol(&BridgeProtocol::Native, &false);
    assert!(!client.is_bridge_protocol_enabled(&BridgeProtocol::Native));
}

#[test]
fn test_layerzero_remote_execution_immediate() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, target_id, target_client) = setup_test(&env, 100, 0);

    let source_chain_id = 200u32;
    let remote_sender = admin.to_xdr(&env);
    client.set_trusted_sender(&source_chain_id, &remote_sender, &true);
    assert!(client.is_trusted_sender(&source_chain_id, &remote_sender));

    let function = symbol_short!("upd_val");
    let args: Vec<Val> = vec![&env, 42u32.into_val(&env)];
    let execution_id = BytesN::from_array(&env, &[7u8; 32]);
    let payload = client.encode_execution_payload(&target_id, &function, &args, &execution_id);

    let message = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 100,
        sequence: 1,
        sender: remote_sender,
        action_type: GovernanceActionType::RemoteExecution,
        payload,
        timestamp: env.ledger().timestamp(),
    };

    let proof = client.layerzero_proof(&source_chain_id, &100);
    let status = client.receive_and_execute(
        &message,
        &proof,
        &target_id,
        &function,
        &args,
        &execution_id,
    );
    assert_eq!(status, ExecutionStatus::Executed);
    assert_eq!(target_client.get_val(), 42);

    let rec = client.get_execution(&execution_id).unwrap();
    assert_eq!(rec.status, ExecutionStatus::Executed);
    assert_eq!(rec.source_chain_id, source_chain_id);
}

#[test]
fn test_wormhole_remote_execution_queued_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let (admin, client, target_id, target_client) = setup_test(&env, 100, 500);

    let source_chain_id = 300u32;
    let remote_sender = admin.to_xdr(&env);
    client.set_trusted_sender(&source_chain_id, &remote_sender, &true);

    let function = symbol_short!("upd_val");
    let args: Vec<Val> = vec![&env, 99u32.into_val(&env)];
    let execution_id = BytesN::from_array(&env, &[9u8; 32]);
    let payload = client.encode_execution_payload(&target_id, &function, &args, &execution_id);

    let message = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 100,
        sequence: 1,
        sender: remote_sender,
        action_type: GovernanceActionType::RemoteExecution,
        payload,
        timestamp: 1_000,
    };

    let proof = client.wormhole_proof(&13, &source_chain_id);
    let status = client.receive_and_execute(
        &message,
        &proof,
        &target_id,
        &function,
        &args,
        &execution_id,
    );
    assert_eq!(status, ExecutionStatus::Queued);
    assert_eq!(target_client.get_val(), 0); // Not executed yet

    // Execution not ready before delay elapses
    let not_ready = client.try_execute_queued(&execution_id);
    assert!(not_ready.is_err());

    // Advance time past delay
    env.ledger().set_timestamp(1_501);
    client.execute_queued(&execution_id);
    assert_eq!(target_client.get_val(), 99);

    let rec = client.get_execution(&execution_id).unwrap();
    assert_eq!(rec.status, ExecutionStatus::Executed);
}

#[test]
fn test_cancel_queued_execution() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let (admin, client, target_id, _) = setup_test(&env, 100, 500);

    let source_chain_id = 300u32;
    let remote_sender = admin.to_xdr(&env);
    client.set_trusted_sender(&source_chain_id, &remote_sender, &true);

    let function = symbol_short!("upd_val");
    let args: Vec<Val> = vec![&env, 77u32.into_val(&env)];
    let execution_id = BytesN::from_array(&env, &[11u8; 32]);
    let payload = client.encode_execution_payload(&target_id, &function, &args, &execution_id);

    let message = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 100,
        sequence: 1,
        sender: remote_sender,
        action_type: GovernanceActionType::RemoteExecution,
        payload,
        timestamp: 1_000,
    };

    let proof = client.native_proof();
    client.receive_and_execute(
        &message,
        &proof,
        &target_id,
        &function,
        &args,
        &execution_id,
    );

    client.cancel_queued(&execution_id);
    let rec = client.get_execution(&execution_id).unwrap();
    assert_eq!(rec.status, ExecutionStatus::Cancelled);

    // Cannot execute cancelled
    env.ledger().set_timestamp(1_600);
    assert!(client.try_execute_queued(&execution_id).is_err());
}

#[test]
fn test_state_sync_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, _, _) = setup_test(&env, 10, 0);

    let source_chain_id = 20u32;
    let remote_sender = admin.to_xdr(&env);
    client.set_trusted_sender(&source_chain_id, &remote_sender, &true);

    let proposal_id = 42u32;
    let status = 2u32; // Passed
    let for_votes = 100_000i128;
    let against_votes = 5_000i128;
    let voting_closed = true;

    let payload = client.encode_proposal_payload(
        &proposal_id,
        &status,
        &for_votes,
        &against_votes,
        &voting_closed,
    );

    let message = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 10,
        sequence: 1,
        sender: remote_sender,
        action_type: GovernanceActionType::ProposalStateSync,
        payload,
        timestamp: env.ledger().timestamp(),
    };

    let proof = client.layerzero_proof(&source_chain_id, &10);
    client.sync_proposal_state(
        &message,
        &proof,
        &proposal_id,
        &status,
        &for_votes,
        &against_votes,
        &voting_closed,
    );

    let synced = client.get_synced_proposal(&source_chain_id, &proposal_id).unwrap();
    assert_eq!(synced.proposal_id, proposal_id);
    assert_eq!(synced.status, status);
    assert_eq!(synced.for_votes, for_votes);
    assert_eq!(synced.against_votes, against_votes);
    assert!(synced.voting_closed);
}

#[test]
fn test_state_sync_voting_power() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, _, _) = setup_test(&env, 10, 0);

    let source_chain_id = 30u32;
    let remote_sender = admin.to_xdr(&env);
    client.set_trusted_sender(&source_chain_id, &remote_sender, &true);

    let voter = Address::generate(&env);
    let voting_units = 500_000i128;
    let snapshot_id = 101u64;

    let payload = client.encode_voting_power_payload(&voter, &voting_units, &snapshot_id);

    let message = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 10,
        sequence: 1,
        sender: remote_sender,
        action_type: GovernanceActionType::VotingPowerSync,
        payload,
        timestamp: env.ledger().timestamp(),
    };

    let proof = client.wormhole_proof(&15, &source_chain_id);
    client.sync_voting_power(
        &message,
        &proof,
        &voter,
        &voting_units,
        &snapshot_id,
    );

    let synced = client
        .get_synced_voting_power(&source_chain_id, &voter, &snapshot_id)
        .unwrap();
    assert_eq!(synced.voter, voter);
    assert_eq!(synced.voting_units, voting_units);
    assert_eq!(synced.snapshot_id, snapshot_id);
}

#[test]
fn test_state_sync_parameters_and_state_root() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, _, _) = setup_test(&env, 10, 0);

    let source_chain_id = 40u32;
    let remote_sender = admin.to_xdr(&env);
    client.set_trusted_sender(&source_chain_id, &remote_sender, &true);

    // Sync parameter
    let param_key = symbol_short!("fee_bps");
    let param_val = 25i128;
    let p_payload = client.encode_parameter_payload(&param_key, &param_val);
    let p_msg = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 10,
        sequence: 1,
        sender: remote_sender.clone(),
        action_type: GovernanceActionType::ParameterSync,
        payload: p_payload,
        timestamp: env.ledger().timestamp(),
    };
    client.sync_parameter(
        &p_msg,
        &client.native_proof(),
        &param_key,
        &param_val,
    );
    let synced_p = client.get_synced_parameter(&param_key).unwrap();
    assert_eq!(synced_p.param_value, 25);

    // Sync state root
    let root = BytesN::from_array(&env, &[0xab; 32]);
    let block_height = 1_000_000u64;
    let r_payload = client.encode_state_root_payload(&source_chain_id, &block_height, &root);
    let r_msg = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 10,
        sequence: 2,
        sender: remote_sender,
        action_type: GovernanceActionType::StateRootSync,
        payload: r_payload,
        timestamp: env.ledger().timestamp(),
    };
    client.sync_state_root(
        &r_msg,
        &client.native_proof(),
        &source_chain_id,
        &block_height,
        &root,
    );
    let synced_r = client.get_synced_state_root(&source_chain_id, &block_height).unwrap();
    assert_eq!(synced_r.root, root);
    let latest_r = client.get_latest_state_root(&source_chain_id).unwrap();
    assert_eq!(latest_r.block_height, block_height);
}

#[test]
fn test_outbound_dispatch_and_broadcast() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, target_id, _) = setup_test(&env, 1, 0);

    let destination_chain = 200u32;
    let exec_id = BytesN::from_array(&env, &[0xcd; 32]);

    // Outbound dispatch execution
    let msg1 = client.dispatch_remote_execution(
        &destination_chain,
        &BridgeProtocol::LayerZero,
        &target_id.to_xdr(&env),
        &symbol_short!("foo"),
        &Bytes::new(&env),
        &exec_id,
    );
    assert_eq!(msg1.sequence, 1);
    assert_eq!(msg1.destination_chain_id, destination_chain);

    // Outbound broadcast proposal
    let msg2 = client.broadcast_proposal_state(
        &destination_chain,
        &BridgeProtocol::Wormhole,
        &12,
        &1,
        &50_000,
        &1_000,
        &false,
    );
    assert_eq!(msg2.sequence, 2);

    assert_eq!(client.get_outbound_sequence(&destination_chain), 2);
}

#[test]
fn test_anti_replay_and_untrusted_sender() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, target_id, _) = setup_test(&env, 100, 0);

    let source_chain_id = 200u32;
    let untrusted_sender = admin.to_xdr(&env);

    let function = symbol_short!("upd_val");
    let args: Vec<Val> = vec![&env, 42u32.into_val(&env)];
    let execution_id = BytesN::from_array(&env, &[7u8; 32]);
    let payload = client.encode_execution_payload(&target_id, &function, &args, &execution_id);

    let message = CrossChainGovMessage {
        source_chain_id,
        destination_chain_id: 100,
        sequence: 1,
        sender: untrusted_sender.clone(),
        action_type: GovernanceActionType::RemoteExecution,
        payload,
        timestamp: env.ledger().timestamp(),
    };
    let proof = client.native_proof();

    // Rejects untrusted sender
    let res = client.try_receive_and_execute(
        &message,
        &proof,
        &target_id,
        &function,
        &args,
        &execution_id,
    );
    assert!(res.is_err());

    // Trust sender and succeed
    client.set_trusted_sender(&source_chain_id, &untrusted_sender, &true);
    let ok = client.receive_and_execute(
        &message,
        &proof,
        &target_id,
        &function,
        &args,
        &execution_id,
    );
    assert_eq!(ok, ExecutionStatus::Executed);

    // Replay with same message rejected
    let replay = client.try_receive_and_execute(
        &message,
        &proof,
        &target_id,
        &function,
        &args,
        &execution_id,
    );
    assert!(replay.is_err());
}
