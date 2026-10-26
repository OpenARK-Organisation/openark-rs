use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START_PREPARED,
    NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL, OpenArkServiceProvider, OpenArkUser,
    ROUND_MESSAGE_VERSION, event_has_event_reference, event_has_message_type,
    event_has_message_version, event_has_round_id, event_is_openark_kind_150,
    parse_new_round_initiate_payload, parse_new_round_join_payload,
    parse_new_round_start_prepared_payload, parse_new_round_vtxo_tree_accept_payload,
    validate_new_round_start_prepared_event, validate_new_round_vtxo_tree_accept_event,
    validate_new_round_vtxo_tree_proposal_event,
};
use std::time::Duration;
mod support;
use support::{
    TraceRecorder, canonical_join_payload, canonical_start_prepared_payload,
    canonical_vtxo_tree_accept_payload, canonical_vtxo_tree_proposal_payload, fund_wallets_two_btc,
    initialize_default_asp_funding_pool, prepare_onboard_utxos_one_btc, start_relay,
};

fn assert_round_event(
    event: &nostr_sdk::prelude::Event,
    message_type: &str,
    round_id: &str,
    round_ref: Option<&nostr_sdk::prelude::EventId>,
) {
    assert!(event_is_openark_kind_150(event));
    assert!(event_has_message_type(event, message_type));
    if message_type == NEW_ROUND_INITIATE {
        assert!(event_has_message_version(event, ROUND_MESSAGE_VERSION));
        let payload = parse_new_round_initiate_payload(&event.content)
            .expect("initiate payload should parse");
        assert_eq!(payload.closing_block, 910000);
        assert_eq!(payload.recycled_block, 910144);
        assert_eq!(payload.leaf_timeout, 10);
    } else if message_type == NEW_ROUND_JOIN {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else if message_type == NEW_ROUND_VTXO_TREE_PROPOSAL {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(validate_new_round_vtxo_tree_proposal_event(event, round_ref).is_ok());
    } else if message_type == NEW_ROUND_VTXO_TREE_ACCEPT {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else if message_type == NEW_ROUND_PREPARE_START {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else if message_type == NEW_ROUND_START_PREPARED {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else {
        assert!(event_has_round_id(event, round_id));
    }
}

fn is_valid_accept_payload(content: &str) -> bool {
    let Some(payload) = parse_new_round_vtxo_tree_accept_payload(content) else {
        return false;
    };
    payload
        .vtxo_tree_signatures
        .values()
        .all(|sig| sig.starts_with("BASE64_SIG_"))
}

fn is_valid_prepared_payload(content: &str) -> bool {
    let Some(payload) = parse_new_round_start_prepared_payload(content) else {
        return false;
    };
    payload
        .funding_signatures
        .values()
        .all(|sig| sig.starts_with("BASE64_FUNDING_SIG_"))
        && payload
            .forfeit_signatures
            .values()
            .all(|sig| sig.starts_with("BASE64_FORFEIT_SIG_"))
        && payload.recycle_signature.starts_with("BASE64_RECYCLE_SIG_")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_missing_new_round_join_blocks_phase() {
    const ROUND_ID: &str = "r-2026-02-24-004-a";
    let mut trace = TraceRecorder::new("step4_missing_join", Some(ROUND_ID));
    println!("[test] failure_missing_new_round_join_blocks_phase started");

    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let alice = OpenArkUser::connect(&relay_url)
        .await
        .expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");
    let carol = OpenArkUser::connect(&relay_url)
        .await
        .expect("carol connect");
    fund_wallets_two_btc(&[
        (
            "Steve",
            provider.wallet_receive_address(),
            provider.wallet_seed_phrase().to_string(),
        ),
        (
            "Alice",
            alice.wallet_receive_address(),
            alice.wallet_seed_phrase().to_string(),
        ),
        (
            "Bob",
            bob.wallet_receive_address(),
            bob.wallet_seed_phrase().to_string(),
        ),
        (
            "Carol",
            carol.wallet_receive_address(),
            carol.wallet_seed_phrase().to_string(),
        ),
    ])
    .await;
    let _asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let prepared_onboard = prepare_onboard_utxos_one_btc(
        &mut trace,
        &[
            (
                "Alice",
                alice.wallet_receive_address(),
                alice.wallet_seed_phrase().to_string(),
            ),
            (
                "Bob",
                bob.wallet_receive_address(),
                bob.wallet_seed_phrase().to_string(),
            ),
            (
                "Carol",
                carol.wallet_receive_address(),
                carol.wallet_seed_phrase().to_string(),
            ),
        ],
    )
    .await;
    for name in ["Alice", "Bob", "Carol"] {
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist");
        assert_eq!(prepared.amount_sats, 100_000_000);
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144, 10)
        .await
        .expect("initiate send");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    let mut round_ref: Option<nostr_sdk::prelude::EventId> = None;
    for (name, user) in [("Alice", &alice), ("Bob", &bob), ("Carol", &carol)] {
        let initiate = user
            .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
        println!("[{name}] received initiate id={}", initiate.id);
        assert_round_event(&initiate, NEW_ROUND_INITIATE, ROUND_ID, None);
        if let Some(existing) = round_ref.as_ref() {
            assert_eq!(existing, &initiate.id);
        } else {
            round_ref = Some(initiate.id);
        }
    }
    let round_ref = round_ref.expect("round ref should be present");
    let alice_join_payload = canonical_join_payload(
        "Alice",
        alice.round_user_pubkey_hex(),
        prepared_onboard
            .get("Alice")
            .expect("Alice onboard utxo should exist")
            .outpoint
            .clone(),
    );
    let bob_join_payload = canonical_join_payload(
        "Bob",
        bob.round_user_pubkey_hex(),
        prepared_onboard
            .get("Bob")
            .expect("Bob onboard utxo should exist")
            .outpoint
            .clone(),
    );

    alice
        .send_new_round_join(&round_ref, &alice_join_payload)
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &bob_join_payload)
        .await
        .expect("bob join");
    println!("[Alice/Bob] published {NEW_ROUND_JOIN}; [Carol] intentionally silent");

    let mut joins = 0usize;
    for (name, pubkey) in [("Alice", alice.pubkey), ("Bob", bob.pubkey)] {
        let join = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("provider wait join")
            .expect("provider receive join");
        println!("[Steve] received join from {name} id={}", join.id);
        assert_round_event(&join, NEW_ROUND_JOIN, ROUND_ID, Some(&round_ref));
        let join_payload =
            parse_new_round_join_payload(&join.content).expect("join payload should parse");
        assert_eq!(join_payload.onboard.len(), 1);
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist for join sender");
        assert_eq!(prepared.amount_sats, 100_000_000);
        assert_eq!(join_payload.onboard[0].outpoint, prepared.outpoint);
        assert_eq!(join_payload.offboard.len(), 0);
        joins += 1;
    }

    let missing = provider
        .wait_for_new_round_join_event(carol.pubkey, &round_ref, Duration::from_secs(1))
        .await
        .expect("provider wait missing join");

    assert!(missing.is_none());
    println!("[Steve] BLOCKED: missing join from Carol before timeout");
    assert_eq!(joins, 2);
    trace.note("blocked", "missing join from Carol before timeout");
    trace.finish(
        "passed",
        serde_json::json!({"scenario":"missing_join","joins_received":joins}),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_invalid_vtxo_tree_accept_token_blocks_phase() {
    const ROUND_ID: &str = "r-2026-02-24-004-b";
    let mut trace = TraceRecorder::new("step4_invalid_accept", Some(ROUND_ID));
    println!("[test] failure_invalid_vtxo_tree_accept_token_blocks_phase started");

    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let alice = OpenArkUser::connect(&relay_url)
        .await
        .expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");
    fund_wallets_two_btc(&[
        (
            "Steve",
            provider.wallet_receive_address(),
            provider.wallet_seed_phrase().to_string(),
        ),
        (
            "Alice",
            alice.wallet_receive_address(),
            alice.wallet_seed_phrase().to_string(),
        ),
        (
            "Bob",
            bob.wallet_receive_address(),
            bob.wallet_seed_phrase().to_string(),
        ),
    ])
    .await;
    let _asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let prepared_onboard = prepare_onboard_utxos_one_btc(
        &mut trace,
        &[
            (
                "Alice",
                alice.wallet_receive_address(),
                alice.wallet_seed_phrase().to_string(),
            ),
            (
                "Bob",
                bob.wallet_receive_address(),
                bob.wallet_seed_phrase().to_string(),
            ),
        ],
    )
    .await;
    for name in ["Alice", "Bob"] {
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist");
        assert_eq!(prepared.amount_sats, 100_000_000);
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144, 10)
        .await
        .expect("initiate send");
    let mut round_ref: Option<nostr_sdk::prelude::EventId> = None;
    for user in [&alice, &bob] {
        let initiate = user
            .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
        assert_round_event(&initiate, NEW_ROUND_INITIATE, ROUND_ID, None);
        if let Some(existing) = round_ref.as_ref() {
            assert_eq!(existing, &initiate.id);
        } else {
            round_ref = Some(initiate.id);
        }
    }
    let round_ref = round_ref.expect("round ref should be present");
    let alice_join_payload = canonical_join_payload(
        "Alice",
        alice.round_user_pubkey_hex(),
        prepared_onboard
            .get("Alice")
            .expect("Alice onboard utxo should exist")
            .outpoint
            .clone(),
    );
    let bob_join_payload = canonical_join_payload(
        "Bob",
        bob.round_user_pubkey_hex(),
        prepared_onboard
            .get("Bob")
            .expect("Bob onboard utxo should exist")
            .outpoint
            .clone(),
    );

    alice
        .send_new_round_join(&round_ref, &alice_join_payload)
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &bob_join_payload)
        .await
        .expect("bob join");

    for (name, pubkey) in [("Alice", alice.pubkey), ("Bob", bob.pubkey)] {
        let join = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("provider wait join")
            .expect("provider receive join");
        let parsed =
            parse_new_round_join_payload(&join.content).expect("join payload should parse");
        assert_eq!(parsed.onboard.len(), 1);
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist for join sender");
        assert_eq!(prepared.amount_sats, 100_000_000);
        assert_eq!(parsed.onboard[0].outpoint, prepared.outpoint);
    }

    let proposal_payload = canonical_vtxo_tree_proposal_payload();
    provider
        .send_new_round_vtxo_tree_proposal(&round_ref, &proposal_payload)
        .await
        .expect("proposal send");
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    let alice_proposal = alice
        .wait_for_new_round_vtxo_tree_proposal_event(
            provider.pubkey,
            &round_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("alice wait proposal")
        .expect("alice proposal");
    let bob_proposal = bob
        .wait_for_new_round_vtxo_tree_proposal_event(
            provider.pubkey,
            &round_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("bob wait proposal")
        .expect("bob proposal");
    assert_eq!(alice_proposal.id, bob_proposal.id);
    let proposal_ref = alice_proposal.id;

    let alice_accept_payload = canonical_vtxo_tree_accept_payload("Alice");
    let mut bob_accept_payload = canonical_vtxo_tree_accept_payload("Bob");
    bob_accept_payload.vtxo_tree_signatures.insert(
        "not-a-hex-txid:0".to_string(),
        "BASE64_SIG_INVALID_Bob".to_string(),
    );

    alice
        .send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &alice_accept_payload)
        .await
        .expect("alice accept");
    bob.send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &bob_accept_payload)
        .await
        .expect("bob invalid accept");

    let alice_accept = provider
        .wait_for_new_round_vtxo_tree_accept_event(
            alice.pubkey,
            &round_ref,
            &proposal_ref,
            Duration::from_secs(5),
        )
        .await
        .expect("wait alice accept")
        .expect("receive alice accept");
    let bob_accept = provider
        .wait_for_new_round_vtxo_tree_accept_event(
            bob.pubkey,
            &round_ref,
            &proposal_ref,
            Duration::from_secs(5),
        )
        .await
        .expect("wait bob accept");

    assert_round_event(
        &alice_accept,
        NEW_ROUND_VTXO_TREE_ACCEPT,
        ROUND_ID,
        Some(&round_ref),
    );
    assert!(
        bob_accept.is_none(),
        "invalid accept should be filtered out"
    );

    let mut valid = 0usize;
    if validate_new_round_vtxo_tree_accept_event(&alice_accept, &round_ref, &proposal_ref).is_ok()
        && is_valid_accept_payload(&alice_accept.content)
    {
        valid += 1;
    }
    if let Some(bob_accept) = bob_accept.as_ref() {
        if validate_new_round_vtxo_tree_accept_event(bob_accept, &round_ref, &proposal_ref).is_ok()
            && is_valid_accept_payload(&bob_accept.content)
        {
            valid += 1;
        }
    }

    assert_eq!(valid, 1);
    println!("[Steve] BLOCKED: invalid vtxo_tree_accept payload token detected");
    trace.note("blocked", "invalid vtxo_tree_accept payload token detected");
    trace.finish(
        "passed",
        serde_json::json!({"scenario":"invalid_accept","valid_accept_count":valid}),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_invalid_start_prepared_token_blocks_activation() {
    const ROUND_ID: &str = "r-2026-02-24-004-c";
    let mut trace = TraceRecorder::new("step4_invalid_start_prepared", Some(ROUND_ID));
    println!("[test] failure_invalid_start_prepared_token_blocks_activation started");

    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let alice = OpenArkUser::connect(&relay_url)
        .await
        .expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");
    fund_wallets_two_btc(&[
        (
            "Steve",
            provider.wallet_receive_address(),
            provider.wallet_seed_phrase().to_string(),
        ),
        (
            "Alice",
            alice.wallet_receive_address(),
            alice.wallet_seed_phrase().to_string(),
        ),
        (
            "Bob",
            bob.wallet_receive_address(),
            bob.wallet_seed_phrase().to_string(),
        ),
    ])
    .await;
    let _asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let prepared_onboard = prepare_onboard_utxos_one_btc(
        &mut trace,
        &[
            (
                "Alice",
                alice.wallet_receive_address(),
                alice.wallet_seed_phrase().to_string(),
            ),
            (
                "Bob",
                bob.wallet_receive_address(),
                bob.wallet_seed_phrase().to_string(),
            ),
        ],
    )
    .await;
    for name in ["Alice", "Bob"] {
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist");
        assert_eq!(prepared.amount_sats, 100_000_000);
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144, 10)
        .await
        .expect("initiate send");
    let mut round_ref: Option<nostr_sdk::prelude::EventId> = None;
    for user in [&alice, &bob] {
        let initiate = user
            .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
        assert_round_event(&initiate, NEW_ROUND_INITIATE, ROUND_ID, None);
        if let Some(existing) = round_ref.as_ref() {
            assert_eq!(existing, &initiate.id);
        } else {
            round_ref = Some(initiate.id);
        }
    }
    let round_ref = round_ref.expect("round ref should be present");
    let alice_join_payload = canonical_join_payload(
        "Alice",
        alice.round_user_pubkey_hex(),
        prepared_onboard
            .get("Alice")
            .expect("Alice onboard utxo should exist")
            .outpoint
            .clone(),
    );
    let bob_join_payload = canonical_join_payload(
        "Bob",
        bob.round_user_pubkey_hex(),
        prepared_onboard
            .get("Bob")
            .expect("Bob onboard utxo should exist")
            .outpoint
            .clone(),
    );

    alice
        .send_new_round_join(&round_ref, &alice_join_payload)
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &bob_join_payload)
        .await
        .expect("bob join");
    for (name, pubkey) in [("Alice", alice.pubkey), ("Bob", bob.pubkey)] {
        let join = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("provider wait join")
            .expect("provider receive join");
        let parsed =
            parse_new_round_join_payload(&join.content).expect("join payload should parse");
        assert_eq!(parsed.onboard.len(), 1);
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist for join sender");
        assert_eq!(prepared.amount_sats, 100_000_000);
        assert_eq!(parsed.onboard[0].outpoint, prepared.outpoint);
    }

    let proposal_payload = canonical_vtxo_tree_proposal_payload();
    provider
        .send_new_round_vtxo_tree_proposal(&round_ref, &proposal_payload)
        .await
        .expect("proposal send");
    let mut proposal_ref: Option<nostr_sdk::prelude::EventId> = None;
    for user in [&alice, &bob] {
        let proposal = user
            .wait_for_new_round_vtxo_tree_proposal_event(
                provider.pubkey,
                &round_ref,
                Duration::from_secs(10),
            )
            .await
            .expect("wait proposal")
            .expect("receive proposal");
        if let Some(existing) = proposal_ref.as_ref() {
            assert_eq!(existing, &proposal.id);
        } else {
            proposal_ref = Some(proposal.id);
        }
    }
    let proposal_ref = proposal_ref.expect("proposal ref should be present");

    let alice_accept_payload = canonical_vtxo_tree_accept_payload("Alice");
    let bob_accept_payload = canonical_vtxo_tree_accept_payload("Bob");
    alice
        .send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &alice_accept_payload)
        .await
        .expect("alice accept");
    bob.send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &bob_accept_payload)
        .await
        .expect("bob accept");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_vtxo_tree_accept_event(
                pubkey,
                &round_ref,
                &proposal_ref,
                Duration::from_secs(5),
            )
            .await
            .expect("wait accept")
            .expect("receive accept");
    }

    provider
        .send_new_round_prepare_start(&round_ref, &proposal_ref)
        .await
        .expect("prepare start send");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    let mut prepare_start_ref: Option<nostr_sdk::prelude::EventId> = None;
    for user in [&alice, &bob] {
        let prepare_start = user
            .wait_for_new_round_prepare_start_event(
                provider.pubkey,
                &round_ref,
                &proposal_ref,
                Duration::from_secs(10),
            )
            .await
            .expect("wait prepare")
            .expect("receive prepare");
        if let Some(existing) = prepare_start_ref.as_ref() {
            assert_eq!(existing, &prepare_start.id);
        } else {
            prepare_start_ref = Some(prepare_start.id);
        }
    }
    let prepare_start_ref = prepare_start_ref.expect("prepare_start ref should be present");

    let alice_start_prepared = canonical_start_prepared_payload("Alice");
    let mut bob_start_prepared = canonical_start_prepared_payload("Bob");
    bob_start_prepared.forfeit_signatures.insert(
        "not-a-hex-txid:0".to_string(),
        "BASE64_FORFEIT_SIG_BAD_Bob".to_string(),
    );
    alice
        .send_new_round_start_prepared(&round_ref, &prepare_start_ref, &alice_start_prepared)
        .await
        .expect("alice prepared");
    bob.send_new_round_start_prepared(&round_ref, &prepare_start_ref, &bob_start_prepared)
        .await
        .expect("bob invalid prepared");

    let alice_prepared = provider
        .wait_for_new_round_start_prepared_event(
            alice.pubkey,
            &round_ref,
            &prepare_start_ref,
            Duration::from_secs(5),
        )
        .await
        .expect("wait alice prepared")
        .expect("receive alice prepared");
    let bob_prepared = provider
        .wait_for_new_round_start_prepared_event(
            bob.pubkey,
            &round_ref,
            &prepare_start_ref,
            Duration::from_secs(5),
        )
        .await
        .expect("wait bob prepared");

    assert_round_event(
        &alice_prepared,
        NEW_ROUND_START_PREPARED,
        ROUND_ID,
        Some(&round_ref),
    );
    assert!(
        validate_new_round_start_prepared_event(&alice_prepared, &round_ref, &prepare_start_ref)
            .is_ok()
    );
    assert!(
        bob_prepared.is_none(),
        "invalid start_prepared should be filtered out"
    );

    let mut valid = 0usize;
    if is_valid_prepared_payload(&alice_prepared.content) {
        valid += 1;
    }
    if let Some(bob_prepared) = bob_prepared.as_ref() {
        if is_valid_prepared_payload(&bob_prepared.content) {
            valid += 1;
        }
    }

    assert_eq!(valid, 1);
    println!("[Steve] BLOCKED: invalid start_prepared payload token detected");
    trace.note("blocked", "invalid start_prepared payload token detected");
    trace.finish(
        "passed",
        serde_json::json!({"scenario":"invalid_start_prepared","valid_prepared_count":valid}),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_late_start_prepared_blocks_activation() {
    const ROUND_ID: &str = "r-2026-02-24-004-d";
    let mut trace = TraceRecorder::new("step4_late_start_prepared", Some(ROUND_ID));
    println!("[test] failure_late_start_prepared_blocks_activation started");

    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let alice = OpenArkUser::connect(&relay_url)
        .await
        .expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");
    fund_wallets_two_btc(&[
        (
            "Steve",
            provider.wallet_receive_address(),
            provider.wallet_seed_phrase().to_string(),
        ),
        (
            "Alice",
            alice.wallet_receive_address(),
            alice.wallet_seed_phrase().to_string(),
        ),
        (
            "Bob",
            bob.wallet_receive_address(),
            bob.wallet_seed_phrase().to_string(),
        ),
    ])
    .await;
    let _asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let prepared_onboard = prepare_onboard_utxos_one_btc(
        &mut trace,
        &[
            (
                "Alice",
                alice.wallet_receive_address(),
                alice.wallet_seed_phrase().to_string(),
            ),
            (
                "Bob",
                bob.wallet_receive_address(),
                bob.wallet_seed_phrase().to_string(),
            ),
        ],
    )
    .await;
    for name in ["Alice", "Bob"] {
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist");
        assert_eq!(prepared.amount_sats, 100_000_000);
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144, 10)
        .await
        .expect("initiate send");
    let mut round_ref: Option<nostr_sdk::prelude::EventId> = None;
    for user in [&alice, &bob] {
        let initiate = user
            .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
        assert_round_event(&initiate, NEW_ROUND_INITIATE, ROUND_ID, None);
        if let Some(existing) = round_ref.as_ref() {
            assert_eq!(existing, &initiate.id);
        } else {
            round_ref = Some(initiate.id);
        }
    }
    let round_ref = round_ref.expect("round ref should be present");
    let alice_join_payload = canonical_join_payload(
        "Alice",
        alice.round_user_pubkey_hex(),
        prepared_onboard
            .get("Alice")
            .expect("Alice onboard utxo should exist")
            .outpoint
            .clone(),
    );
    let bob_join_payload = canonical_join_payload(
        "Bob",
        bob.round_user_pubkey_hex(),
        prepared_onboard
            .get("Bob")
            .expect("Bob onboard utxo should exist")
            .outpoint
            .clone(),
    );

    alice
        .send_new_round_join(&round_ref, &alice_join_payload)
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &bob_join_payload)
        .await
        .expect("bob join");
    for (name, pubkey) in [("Alice", alice.pubkey), ("Bob", bob.pubkey)] {
        let join = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("wait join")
            .expect("receive join");
        let parsed =
            parse_new_round_join_payload(&join.content).expect("join payload should parse");
        assert_eq!(parsed.onboard.len(), 1);
        let prepared = prepared_onboard
            .get(name)
            .expect("prepared onboard utxo should exist for join sender");
        assert_eq!(prepared.amount_sats, 100_000_000);
        assert_eq!(parsed.onboard[0].outpoint, prepared.outpoint);
    }

    let proposal_payload = canonical_vtxo_tree_proposal_payload();
    provider
        .send_new_round_vtxo_tree_proposal(&round_ref, &proposal_payload)
        .await
        .expect("proposal send");
    let mut proposal_ref: Option<nostr_sdk::prelude::EventId> = None;
    for user in [&alice, &bob] {
        let proposal = user
            .wait_for_new_round_vtxo_tree_proposal_event(
                provider.pubkey,
                &round_ref,
                Duration::from_secs(10),
            )
            .await
            .expect("wait proposal")
            .expect("receive proposal");
        if let Some(existing) = proposal_ref.as_ref() {
            assert_eq!(existing, &proposal.id);
        } else {
            proposal_ref = Some(proposal.id);
        }
    }
    let proposal_ref = proposal_ref.expect("proposal ref should be present");

    let alice_accept_payload = canonical_vtxo_tree_accept_payload("Alice");
    let bob_accept_payload = canonical_vtxo_tree_accept_payload("Bob");
    alice
        .send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &alice_accept_payload)
        .await
        .expect("alice accept");
    bob.send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &bob_accept_payload)
        .await
        .expect("bob accept");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_vtxo_tree_accept_event(
                pubkey,
                &round_ref,
                &proposal_ref,
                Duration::from_secs(5),
            )
            .await
            .expect("wait accept")
            .expect("receive accept");
    }

    provider
        .send_new_round_prepare_start(&round_ref, &proposal_ref)
        .await
        .expect("prepare start send");
    let mut prepare_start_ref: Option<nostr_sdk::prelude::EventId> = None;
    for user in [&alice, &bob] {
        let prepare_start = user
            .wait_for_new_round_prepare_start_event(
                provider.pubkey,
                &round_ref,
                &proposal_ref,
                Duration::from_secs(10),
            )
            .await
            .expect("wait prepare")
            .expect("receive prepare");
        if let Some(existing) = prepare_start_ref.as_ref() {
            assert_eq!(existing, &prepare_start.id);
        } else {
            prepare_start_ref = Some(prepare_start.id);
        }
    }
    let prepare_start_ref = prepare_start_ref.expect("prepare_start ref should be present");

    let alice_start_prepared = canonical_start_prepared_payload("Alice");
    alice
        .send_new_round_start_prepared(&round_ref, &prepare_start_ref, &alice_start_prepared)
        .await
        .expect("alice prepared");

    let bob_start_prepared = canonical_start_prepared_payload("Bob");
    let bob_late = async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        bob.send_new_round_start_prepared(&round_ref, &prepare_start_ref, &bob_start_prepared)
            .await
            .expect("bob late prepared send");
        println!("[Bob] late {NEW_ROUND_START_PREPARED} published");
    };

    let alice_prepared = provider
        .wait_for_new_round_start_prepared_event(
            alice.pubkey,
            &round_ref,
            &prepare_start_ref,
            Duration::from_secs(1),
        )
        .await
        .expect("wait alice prepared")
        .expect("receive alice prepared");
    assert_round_event(
        &alice_prepared,
        NEW_ROUND_START_PREPARED,
        ROUND_ID,
        Some(&round_ref),
    );
    assert!(
        validate_new_round_start_prepared_event(&alice_prepared, &round_ref, &prepare_start_ref)
            .is_ok()
    );

    let bob_timeout = provider
        .wait_for_new_round_start_prepared_event(
            bob.pubkey,
            &round_ref,
            &prepare_start_ref,
            Duration::from_secs(1),
        )
        .await
        .expect("wait bob prepared timeout");

    assert!(bob_timeout.is_none());
    println!("[Steve] BLOCKED: late start_prepared from Bob missed timeout window");
    trace.note(
        "blocked",
        "late start_prepared from Bob missed timeout window",
    );

    bob_late.await;
    trace.finish(
        "passed",
        serde_json::json!({"scenario":"late_start_prepared","bob_timeout_observed":true}),
    );
}
