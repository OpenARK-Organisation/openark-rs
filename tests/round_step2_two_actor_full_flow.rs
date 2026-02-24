use nostr_sdk::prelude::Event;
use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START,
    NEW_ROUND_START_PREPARED, NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL,
    OpenArkServiceProvider, OpenArkUser, ROUND_MESSAGE_VERSION, event_event_reference,
    event_has_message_type, event_has_event_reference, event_has_message_version,
    event_has_round_id, event_is_openark_kind_150,
    parse_new_round_initiate_payload, parse_new_round_join_payload, validate_new_round_initiate_event,
    validate_new_round_join_event_reference,
};
use std::time::Duration;
mod support;
use support::{TraceRecorder, canonical_join_payload, start_relay};

const ROUND_ID: &str = "r-2026-02-24-002";

fn assert_round_event(event: &Event, message_type: &str, round_ref: Option<&nostr_sdk::prelude::EventId>) {
    assert!(event_is_openark_kind_150(event));
    assert!(event_has_message_type(event, message_type));
    if message_type == NEW_ROUND_INITIATE {
        assert!(event_has_message_version(event, ROUND_MESSAGE_VERSION));
        assert!(validate_new_round_initiate_event(event).is_ok());
        let payload = parse_new_round_initiate_payload(&event.content)
            .expect("initiate payload should parse");
        assert_eq!(payload.closing_block, 910000);
        assert_eq!(payload.recycled_block, 910144);
    } else if message_type == NEW_ROUND_JOIN {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
        assert!(validate_new_round_join_event_reference(event, round_ref).is_ok());
    } else {
        assert!(event_has_round_id(event, ROUND_ID));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn step2_two_actor_kind150_full_phase_flow() {
    let mut trace = TraceRecorder::new("round_step2_two_actor_full_flow", Some(ROUND_ID));
    println!("[test] step2_two_actor_kind150_full_phase_flow started");
    let (_relay_container, relay_url) = start_relay().await;
    println!("[test] using relay {relay_url}");

    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    println!("[Steve] connected with pubkey={}", provider.pubkey);

    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    println!("[Alice] connected with pubkey={}", user.pubkey);

    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("[test] clients settled, executing phase sequence");

    let mut observed_order: Vec<&'static str> = Vec::new();

    println!("[phase] Round Announcement");
    trace.note("round_announcement", "provider publishing initiate");
    provider
        .send_new_round_initiate(910000, 910144)
        .await
        .expect("provider should send new_round_initiate");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    let initiate = user
        .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
        .await
        .expect("user should wait for initiate")
        .expect("user should receive initiate");
    trace.event("Alice", "received initiate", &initiate);
    assert_round_event(&initiate, NEW_ROUND_INITIATE, None);
    let round_ref = initiate.id;
    println!("[round] reference={round_ref}");
    observed_order.push(NEW_ROUND_INITIATE);

    let join_payload = canonical_join_payload("Alice");
    user.send_new_round_join(&round_ref, &join_payload)
        .await
        .expect("user should send join");
    println!("[Alice] published {NEW_ROUND_JOIN}");

    let join = provider
        .wait_for_new_round_join_event(user.pubkey, &round_ref, Duration::from_secs(10))
        .await
        .expect("provider should wait for join")
        .expect("provider should receive join");
    trace.event("Steve", "received join", &join);
    assert_round_event(&join, NEW_ROUND_JOIN, Some(&round_ref));
    println!(
        "[round] join_event_reference={}",
        event_event_reference(&join).expect("join should have e reference")
    );
    let join_payload = parse_new_round_join_payload(&join.content)
        .expect("join content should parse as canonical JSON payload");
    assert_eq!(join_payload.onboard.len(), 1);
    assert_eq!(join_payload.onboard[0].outpoint, "alice:0");
    assert_eq!(join_payload.offboard.len(), 0);
    assert_eq!(join_payload.keys.user_pubkey, "Alice-user-pubkey");
    assert_eq!(join_payload.keys.agent_pubkey, "Alice-agent-pubkey");
    observed_order.push(NEW_ROUND_JOIN);

    println!("[phase] Tree Proposal");
    trace.note("tree_proposal", "provider publishing proposal");
    provider
        .send_new_round_vtxo_tree_proposal(
            ROUND_ID,
            "round_id=r-2026-02-24-002;vtxo_tree=VTXO_ROOT;forfeit_tree=FORFEIT_TREE;recycle=RECYCLE_TX;actor=Steve",
        )
        .await
        .expect("provider should send proposal");
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    let proposal = user
        .wait_for_new_round_vtxo_tree_proposal_event(
            provider.pubkey,
            ROUND_ID,
            Duration::from_secs(10),
        )
        .await
        .expect("user should wait for proposal")
        .expect("user should receive proposal");
    trace.event("Alice", "received proposal", &proposal);
    assert_round_event(&proposal, NEW_ROUND_VTXO_TREE_PROPOSAL, Some(&round_ref));
    assert!(proposal.content.contains("VTXO_ROOT"));
    observed_order.push(NEW_ROUND_VTXO_TREE_PROPOSAL);

    user.send_new_round_vtxo_tree_accept(
        ROUND_ID,
        "round_id=r-2026-02-24-002;vtxo_tree_signatures=VTXO_ROOT_SIGNATURE;actor=Alice",
    )
    .await
    .expect("user should send accept");
    println!("[Alice] published {NEW_ROUND_VTXO_TREE_ACCEPT}");

    let accept = provider
        .wait_for_new_round_vtxo_tree_accept_event(user.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("provider should wait for accept")
        .expect("provider should receive accept");
    trace.event("Steve", "received accept", &accept);
    assert_round_event(&accept, NEW_ROUND_VTXO_TREE_ACCEPT, Some(&round_ref));
    assert!(accept.content.contains("VTXO_ROOT_SIGNATURE"));
    observed_order.push(NEW_ROUND_VTXO_TREE_ACCEPT);

    println!("[phase] Start Preparation");
    trace.note("start_preparation", "provider publishing prepare_start");
    provider
        .send_new_round_prepare_start(
            ROUND_ID,
            "round_id=r-2026-02-24-002;root_binding=VTXO_ROOT;request=FORFEIT+RECYCLE+ONBOARD_SIGS;actor=Steve",
        )
        .await
        .expect("provider should send prepare_start");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    let prepare_start = user
        .wait_for_new_round_prepare_start_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("user should wait for prepare_start")
        .expect("user should receive prepare_start");
    trace.event("Alice", "received prepare_start", &prepare_start);
    assert_round_event(&prepare_start, NEW_ROUND_PREPARE_START, Some(&round_ref));
    assert!(prepare_start.content.contains("VTXO_ROOT"));
    observed_order.push(NEW_ROUND_PREPARE_START);

    user.send_new_round_start_prepared(
        ROUND_ID,
        "round_id=r-2026-02-24-002;forfeit_sig=FORFEIT_SIG_A;recycle_sig=RECYCLE_SIG_A;root_sig=VTXO_ROOT_SIGNATURE;actor=Alice",
    )
    .await
    .expect("user should send start_prepared");
    println!("[Alice] published {NEW_ROUND_START_PREPARED}");

    let prepared = provider
        .wait_for_new_round_start_prepared_event(user.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("provider should wait for start_prepared")
        .expect("provider should receive start_prepared");
    trace.event("Steve", "received start_prepared", &prepared);
    assert_round_event(&prepared, NEW_ROUND_START_PREPARED, Some(&round_ref));
    assert!(prepared.content.contains("FORFEIT_SIG_A"));
    assert!(prepared.content.contains("RECYCLE_SIG_A"));
    assert!(prepared.content.contains("VTXO_ROOT_SIGNATURE"));
    observed_order.push(NEW_ROUND_START_PREPARED);

    println!("[phase] Round Activation");
    trace.note("round_activation", "provider publishing round_start");
    provider
        .send_new_round_start(
            ROUND_ID,
            "round_id=r-2026-02-24-002;round_txid=VTXO_ROOT;status=activated;actor=Steve",
        )
        .await
        .expect("provider should send round_start");
    println!("[Steve] published {NEW_ROUND_START}");

    let start = user
        .wait_for_new_round_start_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("user should wait for round_start")
        .expect("user should receive round_start");
    trace.event("Alice", "received round_start", &start);
    assert_round_event(&start, NEW_ROUND_START, Some(&round_ref));
    assert!(start.content.contains("VTXO_ROOT"));
    observed_order.push(NEW_ROUND_START);

    let expected_order = vec![
        NEW_ROUND_INITIATE,
        NEW_ROUND_JOIN,
        NEW_ROUND_VTXO_TREE_PROPOSAL,
        NEW_ROUND_VTXO_TREE_ACCEPT,
        NEW_ROUND_PREPARE_START,
        NEW_ROUND_START_PREPARED,
        NEW_ROUND_START,
    ];
    assert_eq!(observed_order, expected_order);

    println!("[test] final state: round activated with round_txid=VTXO_ROOT");
    println!("[test] step2_two_actor_kind150_full_phase_flow passed");
    trace.finish(
        "passed",
        serde_json::json!({
            "observed_order": observed_order,
            "expected_order": expected_order
        }),
    );
}
