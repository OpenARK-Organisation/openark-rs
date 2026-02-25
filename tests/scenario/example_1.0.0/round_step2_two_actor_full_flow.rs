use nostr_sdk::prelude::Event;
use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START,
    NEW_ROUND_START_PREPARED, NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL,
    NewRoundStartPayload, OpenArkServiceProvider, OpenArkUser, ROUND_MESSAGE_VERSION, event_event_reference,
    event_has_message_type, event_has_event_reference, event_has_message_version,
    event_is_openark_kind_150,
    parse_new_round_initiate_payload, parse_new_round_join_payload, parse_new_round_start_payload,
    parse_new_round_start_prepared_payload,
    parse_new_round_vtxo_tree_accept_payload, parse_new_round_vtxo_tree_proposal_payload,
    validate_new_round_initiate_event, validate_new_round_join_event_reference,
    validate_new_round_prepare_start_event, validate_new_round_start_event, validate_new_round_start_prepared_event,
    validate_new_round_vtxo_tree_accept_event, validate_new_round_vtxo_tree_proposal_event,
};
use std::time::Duration;
mod support;
use support::{
    PREPARE_FUNDING_OUTPOINT_0, PROPOSAL_VTXO_OUTPOINT_0, TraceRecorder,
    canonical_join_payload, canonical_start_prepared_payload,
    canonical_vtxo_tree_accept_payload, canonical_vtxo_tree_proposal_payload, fund_wallets_two_btc,
    initialize_default_asp_funding_pool, prepare_onboard_utxos_one_btc, start_relay,
};

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
    } else if message_type == NEW_ROUND_VTXO_TREE_PROPOSAL {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(validate_new_round_vtxo_tree_proposal_event(event, round_ref).is_ok());
    } else if message_type == NEW_ROUND_PREPARE_START || message_type == NEW_ROUND_START_PREPARED {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else if message_type == NEW_ROUND_START {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(validate_new_round_start_event(event, round_ref).is_ok());
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
    fund_wallets_two_btc(&[
        ("Steve", provider.wallet_receive_address(), provider.wallet_seed_phrase().to_string()),
        ("Alice", user.wallet_receive_address(), user.wallet_seed_phrase().to_string()),
    ]).await;
    let _asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let prepared_onboard = prepare_onboard_utxos_one_btc(
        &mut trace,
        &[(
            "Alice",
            user.wallet_receive_address(),
            user.wallet_seed_phrase().to_string(),
        )],
    )
    .await;
    let alice_onboard = prepared_onboard
        .get("Alice")
        .expect("Alice onboard utxo should be prepared");
    assert_eq!(
        alice_onboard.amount_sats, 100_000_000,
        "Alice prepared onboard utxo must be exactly 1 BTC"
    );

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

    let join_payload = canonical_join_payload("Alice", alice_onboard.outpoint.clone());
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
    assert_eq!(
        join_payload.onboard[0].outpoint,
        alice_onboard.outpoint,
        "join payload onboard outpoint must match prepared Alice onboard utxo"
    );
    assert_eq!(join_payload.offboard.len(), 0);
    assert_eq!(join_payload.keys.user_pubkey, "Alice-user-pubkey");
    assert_eq!(join_payload.keys.agent_pubkey, "Alice-agent-pubkey");
    observed_order.push(NEW_ROUND_JOIN);

    println!("[phase] Tree Proposal");
    trace.note("tree_proposal", "provider publishing proposal");
    let proposal_payload = canonical_vtxo_tree_proposal_payload();
    provider
        .send_new_round_vtxo_tree_proposal(&round_ref, &proposal_payload)
        .await
        .expect("provider should send proposal");
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    let proposal = user
        .wait_for_new_round_vtxo_tree_proposal_event(
            provider.pubkey,
            &round_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("user should wait for proposal")
        .expect("user should receive proposal");
    trace.event("Alice", "received proposal", &proposal);
    assert_round_event(&proposal, NEW_ROUND_VTXO_TREE_PROPOSAL, Some(&round_ref));
    let parsed_proposal = parse_new_round_vtxo_tree_proposal_payload(&proposal.content)
        .expect("proposal should parse");
    assert_eq!(parsed_proposal.root_tx, proposal_payload.root_tx);
    assert_eq!(parsed_proposal.vtxo_tree, proposal_payload.vtxo_tree);
    assert_eq!(parsed_proposal.forfeit_tree, proposal_payload.forfeit_tree);
    assert_eq!(parsed_proposal.recycle.unilateral_tx, proposal_payload.recycle.unilateral_tx);
    assert_eq!(
        parsed_proposal.recycle.cooperative_tx,
        proposal_payload.recycle.cooperative_tx
    );
    let proposal_ref = proposal.id;
    observed_order.push(NEW_ROUND_VTXO_TREE_PROPOSAL);

    let accept_payload = canonical_vtxo_tree_accept_payload("Alice");
    user.send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &accept_payload)
        .await
        .expect("user should send accept");
    println!("[Alice] published {NEW_ROUND_VTXO_TREE_ACCEPT}");

    let accept = provider
        .wait_for_new_round_vtxo_tree_accept_event(
            user.pubkey,
            &round_ref,
            &proposal_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("provider should wait for accept")
        .expect("provider should receive accept");
    trace.event("Steve", "received accept", &accept);
    assert!(validate_new_round_vtxo_tree_accept_event(&accept, &round_ref, &proposal_ref).is_ok());
    let parsed_accept = parse_new_round_vtxo_tree_accept_payload(&accept.content)
        .expect("accept payload should parse");
    assert_eq!(
        parsed_accept
            .vtxo_tree_signatures
            .get(PROPOSAL_VTXO_OUTPOINT_0)
            .expect("signature for txid_0"),
        "BASE64_SIG_V0_Alice"
    );
    observed_order.push(NEW_ROUND_VTXO_TREE_ACCEPT);

    println!("[phase] Start Preparation");
    trace.note("start_preparation", "provider publishing prepare_start");
    provider
        .send_new_round_prepare_start(&round_ref, &proposal_ref)
        .await
        .expect("provider should send prepare_start");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    let prepare_start = user
        .wait_for_new_round_prepare_start_event(
            provider.pubkey,
            &round_ref,
            &proposal_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("user should wait for prepare_start")
        .expect("user should receive prepare_start");
    trace.event("Alice", "received prepare_start", &prepare_start);
    assert_round_event(&prepare_start, NEW_ROUND_PREPARE_START, Some(&round_ref));
    assert!(validate_new_round_prepare_start_event(&prepare_start, &round_ref, &proposal_ref).is_ok());
    assert!(prepare_start.content.is_empty());
    let prepare_start_ref = prepare_start.id;
    observed_order.push(NEW_ROUND_PREPARE_START);

    let prepared_payload = canonical_start_prepared_payload("Alice");
    user.send_new_round_start_prepared(&round_ref, &prepare_start_ref, &prepared_payload)
        .await
        .expect("user should send start_prepared");
    println!("[Alice] published {NEW_ROUND_START_PREPARED}");

    let prepared = provider
        .wait_for_new_round_start_prepared_event(
            user.pubkey,
            &round_ref,
            &prepare_start_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("provider should wait for start_prepared")
        .expect("provider should receive start_prepared");
    trace.event("Steve", "received start_prepared", &prepared);
    assert_round_event(&prepared, NEW_ROUND_START_PREPARED, Some(&round_ref));
    assert!(
        validate_new_round_start_prepared_event(&prepared, &round_ref, &prepare_start_ref).is_ok()
    );
    let parsed_prepared = parse_new_round_start_prepared_payload(&prepared.content)
        .expect("start_prepared payload should parse");
    assert_eq!(
        parsed_prepared
            .funding_signatures
            .get(PREPARE_FUNDING_OUTPOINT_0)
            .expect("funding signature for txid"),
        "BASE64_FUNDING_SIG_Alice"
    );
    observed_order.push(NEW_ROUND_START_PREPARED);

    println!("[phase] Round Activation");
    trace.note("round_activation", "provider publishing round_start");
    provider
        .send_new_round_start(
            &round_ref,
            &NewRoundStartPayload {
                round_tx: "BASE64_SIGNED_ROOT_TX_002".to_string(),
            },
        )
        .await
        .expect("provider should send round_start");
    println!("[Steve] published {NEW_ROUND_START}");

    let start = user
        .wait_for_new_round_start_event(provider.pubkey, &round_ref, Duration::from_secs(10))
        .await
        .expect("user should wait for round_start")
        .expect("user should receive round_start");
    trace.event("Alice", "received round_start", &start);
    assert_round_event(&start, NEW_ROUND_START, Some(&round_ref));
    let parsed_start = parse_new_round_start_payload(&start.content)
        .expect("start payload should parse");
    assert_eq!(parsed_start.round_tx, "BASE64_SIGNED_ROOT_TX_002");
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

    println!("[test] final state: round activated with round_tx=BASE64_SIGNED_ROOT_TX_002");
    println!("[test] step2_two_actor_kind150_full_phase_flow passed");
    trace.finish(
        "passed",
        serde_json::json!({
            "observed_order": observed_order,
            "expected_order": expected_order
        }),
    );
}
