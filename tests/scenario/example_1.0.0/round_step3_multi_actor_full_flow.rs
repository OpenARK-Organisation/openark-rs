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
    validate_new_round_prepare_start_event, validate_new_round_start_event, validate_new_round_start_prepared_event,
    validate_new_round_vtxo_tree_accept_event, validate_new_round_vtxo_tree_proposal_event,
};
use std::time::Duration;
mod support;
use support::{
    TraceRecorder, canonical_join_payload, canonical_start_prepared_payload,
    canonical_vtxo_tree_accept_payload, canonical_vtxo_tree_proposal_payload, fund_wallets_two_btc,
    estimate_tree_proposal_fee_budget_sats, initialize_default_asp_funding_pool,
    prepare_onboard_utxos_one_btc, start_relay,
};

const ROUND_ID: &str = "r-2026-02-24-003";
const USERS: [&str; 5] = ["Alice", "Bob", "Carol", "Dave", "Eve"];

struct NamedUser {
    name: &'static str,
    user: OpenArkUser,
}

fn assert_round_event(event: &Event, message_type: &str, round_ref: Option<&nostr_sdk::prelude::EventId>) {
    assert!(event_is_openark_kind_150(event));
    assert!(event_has_message_type(event, message_type));
    if message_type == NEW_ROUND_INITIATE {
        assert!(event_has_message_version(event, ROUND_MESSAGE_VERSION));
        let payload = parse_new_round_initiate_payload(&event.content)
            .expect("initiate payload should parse");
        assert_eq!(payload.closing_block, 910000);
        assert_eq!(payload.recycled_block, 910144);
    } else if message_type == NEW_ROUND_JOIN {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn step3_multi_actor_kind150_full_phase_flow() {
    let mut trace = TraceRecorder::new("round_step3_multi_actor_full_flow", Some(ROUND_ID));
    println!("[test] step3_multi_actor_kind150_full_phase_flow started");
    let (_relay_container, relay_url) = start_relay().await;
    println!("[test] using relay {relay_url}");

    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    println!("[Steve] connected with pubkey={}", provider.pubkey);

    let mut users: Vec<NamedUser> = Vec::new();
    for name in USERS {
        let user = OpenArkUser::connect(&relay_url)
            .await
            .expect("user should connect");
        println!("[{name}] connected with pubkey={}", user.pubkey);
        users.push(NamedUser { name, user });
    }
    let mut wallets = vec![
        ("Steve", provider.wallet_receive_address(), provider.wallet_seed_phrase().to_string()),
    ];
    for named in &users {
        wallets.push((
            named.name,
            named.user.wallet_receive_address(),
            named.user.wallet_seed_phrase().to_string(),
        ));
    }
    fund_wallets_two_btc(&wallets).await;
    let mut asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let asp_pool_initial_available_count = asp_pool.available_count();
    let asp_pool_initial_available_value_sats = asp_pool.available_value_sats();
    let onboard_wallets: Vec<(&str, String, String)> = users
        .iter()
        .map(|named| {
            (
                named.name,
                named.user.wallet_receive_address(),
                named.user.wallet_seed_phrase().to_string(),
            )
        })
        .collect();
    let prepared_onboard = prepare_onboard_utxos_one_btc(&mut trace, &onboard_wallets).await;
    for named in &users {
        let prepared = prepared_onboard
            .get(named.name)
            .expect("prepared onboard utxo should exist for each user");
        assert_eq!(
            prepared.amount_sats, 100_000_000,
            "prepared onboard amount should be exactly 1 BTC"
        );
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("[test] clients settled, executing multi-actor phase sequence");

    let mut announcement_joins = 0usize;
    let mut tree_accepts = 0usize;
    let mut prepared_signals = 0usize;
    let mut activated_receipts = 0usize;

    println!("[phase] Round Announcement");
    trace.note("round_announcement", "provider publishing initiate");
    provider
        .send_new_round_initiate(910000, 910144)
        .await
        .expect("provider should send new_round_initiate");
    println!("[Steve] published {NEW_ROUND_INITIATE}");
    let mut round_ref: Option<nostr_sdk::prelude::EventId> = None;

    for named in &users {
        let initiate = named
            .user
            .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
            .await
            .expect("user should wait for initiate")
            .expect("user should receive initiate");
        trace.event(named.name, "received initiate", &initiate);
        assert_round_event(&initiate, NEW_ROUND_INITIATE, None);
        if let Some(existing) = round_ref.as_ref() {
            assert_eq!(existing, &initiate.id);
        } else {
            round_ref = Some(initiate.id);
            println!("[round] reference={}", initiate.id);
        }
        let round_ref = round_ref.as_ref().expect("round ref should be present");

        let prepared = prepared_onboard
            .get(named.name)
            .expect("prepared onboard utxo should exist for join sender");
        let join_payload = canonical_join_payload(named.name, prepared.outpoint.clone());
        named
            .user
            .send_new_round_join(round_ref, &join_payload)
            .await
            .expect("user should send join");
        println!("[{}] published {NEW_ROUND_JOIN}", named.name);
        let join = provider
            .wait_for_new_round_join_event(named.user.pubkey, round_ref, Duration::from_secs(10))
            .await
            .expect("provider should wait for join")
            .expect("provider should receive join");
        trace.event("Steve", &format!("received join from {}", named.name), &join);
        assert_round_event(&join, NEW_ROUND_JOIN, Some(round_ref));
        let join_payload = parse_new_round_join_payload(&join.content)
            .expect("join payload should parse");
        assert_eq!(join_payload.onboard.len(), 1);
        assert_eq!(
            join_payload.onboard[0].outpoint,
            prepared.outpoint
        );
        assert_eq!(join_payload.offboard.len(), 0);
        assert_eq!(
            join_payload.keys.user_pubkey,
            format!("{}-user-pubkey", named.name)
        );
        assert_eq!(
            join_payload.keys.agent_pubkey,
            format!("{}-agent-pubkey", named.name)
        );
        println!(
            "[round] join_event_reference={} actor={}",
            event_event_reference(&join).expect("join should have e reference"),
            named.name
        );
        announcement_joins += 1;
    }
    assert_eq!(announcement_joins, users.len());
    println!("[phase] Round Announcement complete: joins={announcement_joins}");

    println!("[phase] Tree Proposal");
    trace.note("tree_proposal", "provider publishing proposal");
    let proposal_fee_budget_sats = estimate_tree_proposal_fee_budget_sats(users.len());
    let reserved_pool_outpoints = asp_pool.reserve_for_amount(proposal_fee_budget_sats);
    trace.note(
        "asp_pool_reserve",
        &format!(
            "phase=tree_proposal fee_budget_sats={} reserved_outpoints={:?}",
            proposal_fee_budget_sats, reserved_pool_outpoints
        ),
    );
    println!(
        "[asp-pool] reserved_for_tree_proposal fee_budget_sats={} reserved_count={} reserved_value_sats={}",
        proposal_fee_budget_sats,
        reserved_pool_outpoints.len(),
        asp_pool.reserved_value_sats()
    );
    let proposal_payload = canonical_vtxo_tree_proposal_payload();
    let round_ref_for_proposal = round_ref.as_ref().expect("round ref should be present");
    provider
        .send_new_round_vtxo_tree_proposal(round_ref_for_proposal, &proposal_payload)
        .await
        .expect("provider should send proposal");
    asp_pool.consume(&reserved_pool_outpoints);
    trace.note(
        "asp_pool_consume",
        &format!(
            "phase=tree_proposal consumed_outpoints={:?} spent_value_sats={}",
            reserved_pool_outpoints,
            asp_pool.spent_value_sats()
        ),
    );
    println!(
        "[asp-pool] consumed_for_tree_proposal spent_count={} spent_value_sats={}",
        asp_pool.spent_count(),
        asp_pool.spent_value_sats()
    );
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    let mut proposal_ref: Option<nostr_sdk::prelude::EventId> = None;
    for named in &users {
        let proposal = named
            .user
            .wait_for_new_round_vtxo_tree_proposal_event(
                provider.pubkey,
                round_ref_for_proposal,
                Duration::from_secs(10),
            )
            .await
            .expect("user should wait for proposal")
            .expect("user should receive proposal");
        trace.event(named.name, "received proposal", &proposal);
        assert_round_event(
            &proposal,
            NEW_ROUND_VTXO_TREE_PROPOSAL,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        let parsed_proposal = parse_new_round_vtxo_tree_proposal_payload(&proposal.content)
            .expect("proposal should parse");
        assert_eq!(parsed_proposal.root_tx, proposal_payload.root_tx);
        if let Some(existing) = proposal_ref.as_ref() {
            assert_eq!(existing, &proposal.id);
        } else {
            proposal_ref = Some(proposal.id);
        }

        let accept_payload = canonical_vtxo_tree_accept_payload(named.name);
        named
            .user
            .send_new_round_vtxo_tree_accept(
                round_ref_for_proposal,
                proposal_ref.as_ref().expect("proposal ref should be present"),
                &accept_payload,
            )
            .await
            .expect("user should send accept");
        println!("[{}] published {NEW_ROUND_VTXO_TREE_ACCEPT}", named.name);
        let accept = provider
            .wait_for_new_round_vtxo_tree_accept_event(
                named.user.pubkey,
                round_ref_for_proposal,
                proposal_ref.as_ref().expect("proposal ref should be present"),
                Duration::from_secs(10),
            )
            .await
            .expect("provider should wait for accept")
            .expect("provider should receive accept");
        trace.event("Steve", &format!("received accept from {}", named.name), &accept);
        assert!(
            validate_new_round_vtxo_tree_accept_event(
                &accept,
                round_ref_for_proposal,
                proposal_ref.as_ref().expect("proposal ref should be present"),
            )
            .is_ok()
        );
        let parsed_accept = parse_new_round_vtxo_tree_accept_payload(&accept.content)
            .expect("accept payload should parse");
        assert_eq!(parsed_accept.vtxo_tree_signatures.len(), 2);
        tree_accepts += 1;
    }
    assert_eq!(tree_accepts, users.len());
    println!("[phase] Tree Proposal complete: accepts={tree_accepts}");

    println!("[phase] Start Preparation");
    trace.note("start_preparation", "provider publishing prepare_start");
    let round_ref_for_prepare = round_ref.as_ref().expect("round ref should be present");
    provider
        .send_new_round_prepare_start(
            round_ref_for_prepare,
            proposal_ref.as_ref().expect("proposal ref should be present"),
        )
        .await
        .expect("provider should send prepare_start");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    let mut prepare_start_ref: Option<nostr_sdk::prelude::EventId> = None;
    for named in &users {
        let prepare_start = named
            .user
            .wait_for_new_round_prepare_start_event(
                provider.pubkey,
                round_ref_for_prepare,
                proposal_ref.as_ref().expect("proposal ref should be present"),
                Duration::from_secs(10),
            )
            .await
            .expect("user should wait for prepare_start")
            .expect("user should receive prepare_start");
        trace.event(named.name, "received prepare_start", &prepare_start);
        assert_round_event(
            &prepare_start,
            NEW_ROUND_PREPARE_START,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        assert!(
            validate_new_round_prepare_start_event(
                &prepare_start,
                round_ref_for_prepare,
                proposal_ref.as_ref().expect("proposal ref should be present"),
            )
            .is_ok()
        );
        assert!(prepare_start.content.is_empty());
        if let Some(existing) = prepare_start_ref.as_ref() {
            assert_eq!(existing, &prepare_start.id);
        } else {
            prepare_start_ref = Some(prepare_start.id);
        }

        let prepared_payload = canonical_start_prepared_payload(named.name);
        named
            .user
            .send_new_round_start_prepared(
                round_ref_for_prepare,
                prepare_start_ref.as_ref().expect("prepare_start ref should be present"),
                &prepared_payload,
            )
            .await
            .expect("user should send start_prepared");
        println!("[{}] published {NEW_ROUND_START_PREPARED}", named.name);
        let prepared = provider
            .wait_for_new_round_start_prepared_event(
                named.user.pubkey,
                round_ref_for_prepare,
                prepare_start_ref.as_ref().expect("prepare_start ref should be present"),
                Duration::from_secs(10),
            )
            .await
            .expect("provider should wait for start_prepared")
            .expect("provider should receive start_prepared");
        trace.event(
            "Steve",
            &format!("received start_prepared from {}", named.name),
            &prepared,
        );
        assert_round_event(
            &prepared,
            NEW_ROUND_START_PREPARED,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        assert!(
            validate_new_round_start_prepared_event(
                &prepared,
                round_ref_for_prepare,
                prepare_start_ref.as_ref().expect("prepare_start ref should be present"),
            )
            .is_ok()
        );
        assert!(parse_new_round_start_prepared_payload(&prepared.content).is_some());
        prepared_signals += 1;
    }
    assert_eq!(prepared_signals, users.len());
    println!("[phase] Start Preparation complete: prepared_signals={prepared_signals}");

    println!("[phase] Round Activation");
    trace.note("round_activation", "provider publishing round_start");
    provider
        .send_new_round_start(
            round_ref.as_ref().expect("round ref should be present"),
            &NewRoundStartPayload {
                round_tx: "BASE64_SIGNED_ROOT_TX_003".to_string(),
            },
        )
        .await
        .expect("provider should send round_start");
    println!("[Steve] published {NEW_ROUND_START}");

    for named in &users {
        let start = named
            .user
            .wait_for_new_round_start_event(provider.pubkey, round_ref.as_ref().expect("round ref should be present"), Duration::from_secs(10))
            .await
            .expect("user should wait for round_start")
            .expect("user should receive round_start");
        trace.event(named.name, "received round_start", &start);
        assert_round_event(
            &start,
            NEW_ROUND_START,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        let parsed_start = parse_new_round_start_payload(&start.content)
            .expect("start payload should parse");
        assert_eq!(parsed_start.round_tx, "BASE64_SIGNED_ROOT_TX_003");
        activated_receipts += 1;
    }
    assert_eq!(activated_receipts, users.len());
    println!("[phase] Round Activation complete: receipts={activated_receipts}");
    trace.note(
        "asp_pool_summary",
        &format!(
            "initial_available_count={} initial_available_value_sats={} consumed_count={} consumed_value_sats={} remaining_count={} remaining_value_sats={}",
            asp_pool_initial_available_count,
            asp_pool_initial_available_value_sats,
            asp_pool.spent_count(),
            asp_pool.spent_value_sats(),
            asp_pool.available_count(),
            asp_pool.available_value_sats()
        ),
    );
    println!(
        "[asp-pool] summary initial_available_count={} initial_available_value_sats={} consumed_count={} consumed_value_sats={} remaining_count={} remaining_value_sats={}",
        asp_pool_initial_available_count,
        asp_pool_initial_available_value_sats,
        asp_pool.spent_count(),
        asp_pool.spent_value_sats(),
        asp_pool.available_count(),
        asp_pool.available_value_sats()
    );
    println!(
        "[asp-pool] final available={} reserved={} spent={} available_value_sats={} spent_value_sats={}",
        asp_pool.available_count(),
        asp_pool.reserved_count(),
        asp_pool.spent_count(),
        asp_pool.available_value_sats(),
        asp_pool.spent_value_sats()
    );

    println!(
        "[test] final state: round activated with {} users (Alice, Bob, Carol, Dave, Eve)",
        users.len()
    );
    println!("[test] step3_multi_actor_kind150_full_phase_flow passed");
    trace.finish(
        "passed",
        serde_json::json!({
            "announcement_joins": announcement_joins,
            "tree_accepts": tree_accepts,
            "prepared_signals": prepared_signals,
            "activated_receipts": activated_receipts
        }),
    );
}
