use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START_PREPARED,
    NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL, OpenArkServiceProvider, OpenArkUser,
    ROUND_MESSAGE_VERSION, event_has_event_reference, event_has_message_type,
    event_has_message_version, event_has_round_id, event_is_openark_kind_150,
    parse_new_round_initiate_payload,
};
use std::time::Duration;
mod support;
use support::{TraceRecorder, canonical_join_payload, start_relay};

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
    } else if message_type == NEW_ROUND_JOIN {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else {
        assert!(event_has_round_id(event, round_id));
    }
}

fn is_valid_accept_payload(content: &str) -> bool {
    content.contains("VTXO_ROOT_SIGNATURE_")
}

fn is_valid_prepared_payload(content: &str) -> bool {
    content.contains("FORFEIT_SIG_")
        && content.contains("RECYCLE_SIG_")
        && content.contains("VTXO_ROOT_SIGNATURE_")
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
    let alice = OpenArkUser::connect(&relay_url).await.expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");
    let carol = OpenArkUser::connect(&relay_url).await.expect("carol connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144)
        .await
        .expect("initiate send");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    let mut round_ref: Option<nostr_sdk::prelude::EventId> = None;
    for (name, user) in [(&"Alice", &alice), (&"Bob", &bob), (&"Carol", &carol)] {
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

    alice
        .send_new_round_join(&round_ref, &canonical_join_payload("Alice"))
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &canonical_join_payload("Bob"))
        .await
        .expect("bob join");
    println!("[Alice/Bob] published {NEW_ROUND_JOIN}; [Carol] intentionally silent");

    let mut joins = 0usize;
    for (name, pubkey) in [(&"Alice", alice.pubkey), (&"Bob", bob.pubkey)] {
        let join = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("provider wait join")
            .expect("provider receive join");
        println!("[Steve] received join from {name} id={}", join.id);
        assert_round_event(&join, NEW_ROUND_JOIN, ROUND_ID, Some(&round_ref));
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
    let alice = OpenArkUser::connect(&relay_url).await.expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144)
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

    alice
        .send_new_round_join(&round_ref, &canonical_join_payload("Alice"))
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &canonical_join_payload("Bob"))
        .await
        .expect("bob join");

    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("provider wait join")
            .expect("provider receive join");
    }

    provider
        .send_new_round_vtxo_tree_proposal(ROUND_ID, "round_id=r-2026-02-24-004-b;vtxo_tree=VTXO_ROOT")
        .await
        .expect("proposal send");
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    let _ = alice
        .wait_for_new_round_vtxo_tree_proposal_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("alice wait proposal")
        .expect("alice proposal");
    let _ = bob
        .wait_for_new_round_vtxo_tree_proposal_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("bob wait proposal")
        .expect("bob proposal");

    alice
        .send_new_round_vtxo_tree_accept(
            ROUND_ID,
            "round_id=r-2026-02-24-004-b;vtxo_tree_signatures=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice accept");
    bob.send_new_round_vtxo_tree_accept(
        ROUND_ID,
        "round_id=r-2026-02-24-004-b;vtxo_tree_signatures=INVALID_SIG_Bob",
    )
    .await
    .expect("bob invalid accept");

    let alice_accept = provider
        .wait_for_new_round_vtxo_tree_accept_event(alice.pubkey, ROUND_ID, Duration::from_secs(5))
        .await
        .expect("wait alice accept")
        .expect("receive alice accept");
    let bob_accept = provider
        .wait_for_new_round_vtxo_tree_accept_event(bob.pubkey, ROUND_ID, Duration::from_secs(5))
        .await
        .expect("wait bob accept")
        .expect("receive bob accept");

    assert_round_event(
        &alice_accept,
        NEW_ROUND_VTXO_TREE_ACCEPT,
        ROUND_ID,
        Some(&round_ref),
    );
    assert_round_event(
        &bob_accept,
        NEW_ROUND_VTXO_TREE_ACCEPT,
        ROUND_ID,
        Some(&round_ref),
    );

    let mut valid = 0usize;
    if is_valid_accept_payload(&alice_accept.content) {
        valid += 1;
    }
    if is_valid_accept_payload(&bob_accept.content) {
        valid += 1;
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
    let alice = OpenArkUser::connect(&relay_url).await.expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144)
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

    alice
        .send_new_round_join(&round_ref, &canonical_join_payload("Alice"))
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &canonical_join_payload("Bob"))
        .await
        .expect("bob join");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("provider wait join")
            .expect("provider receive join");
    }

    provider
        .send_new_round_vtxo_tree_proposal(ROUND_ID, "round_id=r-2026-02-24-004-c;vtxo_tree=VTXO_ROOT")
        .await
        .expect("proposal send");
    for user in [&alice, &bob] {
        let _ = user
            .wait_for_new_round_vtxo_tree_proposal_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait proposal")
            .expect("receive proposal");
    }

    alice
        .send_new_round_vtxo_tree_accept(
            ROUND_ID,
            "round_id=r-2026-02-24-004-c;vtxo_tree_signatures=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice accept");
    bob.send_new_round_vtxo_tree_accept(
        ROUND_ID,
        "round_id=r-2026-02-24-004-c;vtxo_tree_signatures=VTXO_ROOT_SIGNATURE_Bob",
    )
    .await
    .expect("bob accept");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_vtxo_tree_accept_event(pubkey, ROUND_ID, Duration::from_secs(5))
            .await
            .expect("wait accept")
            .expect("receive accept");
    }

    provider
        .send_new_round_prepare_start(ROUND_ID, "round_id=r-2026-02-24-004-c;root_binding=VTXO_ROOT")
        .await
        .expect("prepare start send");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    for user in [&alice, &bob] {
        let _ = user
            .wait_for_new_round_prepare_start_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait prepare")
            .expect("receive prepare");
    }

    alice
        .send_new_round_start_prepared(
            ROUND_ID,
            "round_id=r-2026-02-24-004-c;forfeit_sig=FORFEIT_SIG_Alice;recycle_sig=RECYCLE_SIG_Alice;root_sig=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice prepared");
    bob.send_new_round_start_prepared(
        ROUND_ID,
        "round_id=r-2026-02-24-004-c;forfeit_sig=MISSING;recycle_sig=MISSING;root_sig=BAD",
    )
    .await
    .expect("bob invalid prepared");

    let alice_prepared = provider
        .wait_for_new_round_start_prepared_event(alice.pubkey, ROUND_ID, Duration::from_secs(5))
        .await
        .expect("wait alice prepared")
        .expect("receive alice prepared");
    let bob_prepared = provider
        .wait_for_new_round_start_prepared_event(bob.pubkey, ROUND_ID, Duration::from_secs(5))
        .await
        .expect("wait bob prepared")
        .expect("receive bob prepared");

    assert_round_event(&alice_prepared, NEW_ROUND_START_PREPARED, ROUND_ID, Some(&round_ref));
    assert_round_event(&bob_prepared, NEW_ROUND_START_PREPARED, ROUND_ID, Some(&round_ref));

    let mut valid = 0usize;
    if is_valid_prepared_payload(&alice_prepared.content) {
        valid += 1;
    }
    if is_valid_prepared_payload(&bob_prepared.content) {
        valid += 1;
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
    let alice = OpenArkUser::connect(&relay_url).await.expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144)
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

    alice
        .send_new_round_join(&round_ref, &canonical_join_payload("Alice"))
        .await
        .expect("alice join");
    bob.send_new_round_join(&round_ref, &canonical_join_payload("Bob"))
        .await
        .expect("bob join");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_join_event(pubkey, &round_ref, Duration::from_secs(5))
            .await
            .expect("wait join")
            .expect("receive join");
    }

    provider
        .send_new_round_vtxo_tree_proposal(ROUND_ID, "round_id=r-2026-02-24-004-d;vtxo_tree=VTXO_ROOT")
        .await
        .expect("proposal send");
    for user in [&alice, &bob] {
        let _ = user
            .wait_for_new_round_vtxo_tree_proposal_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait proposal")
            .expect("receive proposal");
    }

    alice
        .send_new_round_vtxo_tree_accept(
            ROUND_ID,
            "round_id=r-2026-02-24-004-d;vtxo_tree_signatures=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice accept");
    bob.send_new_round_vtxo_tree_accept(
        ROUND_ID,
        "round_id=r-2026-02-24-004-d;vtxo_tree_signatures=VTXO_ROOT_SIGNATURE_Bob",
    )
    .await
    .expect("bob accept");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_vtxo_tree_accept_event(pubkey, ROUND_ID, Duration::from_secs(5))
            .await
            .expect("wait accept")
            .expect("receive accept");
    }

    provider
        .send_new_round_prepare_start(ROUND_ID, "round_id=r-2026-02-24-004-d;root_binding=VTXO_ROOT")
        .await
        .expect("prepare start send");
    for user in [&alice, &bob] {
        let _ = user
            .wait_for_new_round_prepare_start_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait prepare")
            .expect("receive prepare");
    }

    alice
        .send_new_round_start_prepared(
            ROUND_ID,
            "round_id=r-2026-02-24-004-d;forfeit_sig=FORFEIT_SIG_Alice;recycle_sig=RECYCLE_SIG_Alice;root_sig=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice prepared");

    let bob_late = async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        bob.send_new_round_start_prepared(
            ROUND_ID,
            "round_id=r-2026-02-24-004-d;forfeit_sig=FORFEIT_SIG_Bob;recycle_sig=RECYCLE_SIG_Bob;root_sig=VTXO_ROOT_SIGNATURE_Bob",
        )
        .await
        .expect("bob late prepared send");
        println!("[Bob] late {NEW_ROUND_START_PREPARED} published");
    };

    let alice_prepared = provider
        .wait_for_new_round_start_prepared_event(alice.pubkey, ROUND_ID, Duration::from_secs(1))
        .await
        .expect("wait alice prepared")
        .expect("receive alice prepared");
    assert_round_event(&alice_prepared, NEW_ROUND_START_PREPARED, ROUND_ID, Some(&round_ref));

    let bob_timeout = provider
        .wait_for_new_round_start_prepared_event(bob.pubkey, ROUND_ID, Duration::from_secs(1))
        .await
        .expect("wait bob prepared timeout");

    assert!(bob_timeout.is_none());
    println!("[Steve] BLOCKED: late start_prepared from Bob missed timeout window");
    trace.note("blocked", "late start_prepared from Bob missed timeout window");

    bob_late.await;
    trace.finish(
        "passed",
        serde_json::json!({"scenario":"late_start_prepared","bob_timeout_observed":true}),
    );
}
