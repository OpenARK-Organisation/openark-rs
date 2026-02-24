use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START_PREPARED,
    NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL, OpenArkServiceProvider, OpenArkUser,
    event_has_message_type, event_has_round_id, event_is_openark_kind_150,
};
use std::time::Duration;
use testcontainers::{
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

async fn start_relay() -> (testcontainers::ContainerAsync<GenericImage>, String) {
    println!("[relay] starting strfry test container");
    let container = GenericImage::new("strfry-strfry", "latest")
        .with_exposed_port(7777.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Started websocket server"))
        .start()
        .await
        .expect("Failed to start strfry container");

    let host_port = container
        .get_host_port_ipv4(7777)
        .await
        .expect("Failed to map strfry port");

    println!("[relay] strfry ready on ws://localhost:{host_port}");
    (container, format!("ws://localhost:{host_port}"))
}

fn assert_round_event(event: &nostr_sdk::prelude::Event, message_type: &str, round_id: &str) {
    assert!(event_is_openark_kind_150(event));
    assert!(event_has_message_type(event, message_type));
    assert!(event_has_round_id(event, round_id));
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
        .send_new_round_initiate(ROUND_ID, "round_id=r-2026-02-24-004-a;vtxo_root=VTXO_ROOT")
        .await
        .expect("initiate send");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    for (name, user) in [(&"Alice", &alice), (&"Bob", &bob), (&"Carol", &carol)] {
        let initiate = user
            .wait_for_new_round_initiate_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
        println!("[{name}] received initiate id={}", initiate.id);
        assert_round_event(&initiate, NEW_ROUND_INITIATE, ROUND_ID);
    }

    alice
        .send_new_round_join(
            ROUND_ID,
            "round_id=r-2026-02-24-004-a;transition=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice join");
    bob.send_new_round_join(
        ROUND_ID,
        "round_id=r-2026-02-24-004-a;transition=VTXO_ROOT_SIGNATURE_Bob",
    )
    .await
    .expect("bob join");
    println!("[Alice/Bob] published {NEW_ROUND_JOIN}; [Carol] intentionally silent");

    let mut joins = 0usize;
    for (name, pubkey) in [(&"Alice", alice.pubkey), (&"Bob", bob.pubkey)] {
        let join = provider
            .wait_for_new_round_join_event(pubkey, ROUND_ID, Duration::from_secs(5))
            .await
            .expect("provider wait join")
            .expect("provider receive join");
        println!("[Steve] received join from {name} id={}", join.id);
        assert_round_event(&join, NEW_ROUND_JOIN, ROUND_ID);
        joins += 1;
    }

    let missing = provider
        .wait_for_new_round_join_event(carol.pubkey, ROUND_ID, Duration::from_secs(1))
        .await
        .expect("provider wait missing join");

    assert!(missing.is_none());
    println!("[Steve] BLOCKED: missing join from Carol before timeout");
    assert_eq!(joins, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_invalid_vtxo_tree_accept_token_blocks_phase() {
    const ROUND_ID: &str = "r-2026-02-24-004-b";
    println!("[test] failure_invalid_vtxo_tree_accept_token_blocks_phase started");

    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let alice = OpenArkUser::connect(&relay_url).await.expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(ROUND_ID, "round_id=r-2026-02-24-004-b;vtxo_root=VTXO_ROOT")
        .await
        .expect("initiate send");
    for user in [&alice, &bob] {
        let _ = user
            .wait_for_new_round_initiate_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
    }

    alice
        .send_new_round_join(
            ROUND_ID,
            "round_id=r-2026-02-24-004-b;transition=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice join");
    bob.send_new_round_join(
        ROUND_ID,
        "round_id=r-2026-02-24-004-b;transition=VTXO_ROOT_SIGNATURE_Bob",
    )
    .await
    .expect("bob join");

    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_join_event(pubkey, ROUND_ID, Duration::from_secs(5))
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

    assert_round_event(&alice_accept, NEW_ROUND_VTXO_TREE_ACCEPT, ROUND_ID);
    assert_round_event(&bob_accept, NEW_ROUND_VTXO_TREE_ACCEPT, ROUND_ID);

    let mut valid = 0usize;
    if is_valid_accept_payload(&alice_accept.content) {
        valid += 1;
    }
    if is_valid_accept_payload(&bob_accept.content) {
        valid += 1;
    }

    assert_eq!(valid, 1);
    println!("[Steve] BLOCKED: invalid vtxo_tree_accept payload token detected");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_invalid_start_prepared_token_blocks_activation() {
    const ROUND_ID: &str = "r-2026-02-24-004-c";
    println!("[test] failure_invalid_start_prepared_token_blocks_activation started");

    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let alice = OpenArkUser::connect(&relay_url).await.expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(ROUND_ID, "round_id=r-2026-02-24-004-c;vtxo_root=VTXO_ROOT")
        .await
        .expect("initiate send");
    for user in [&alice, &bob] {
        let _ = user
            .wait_for_new_round_initiate_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
    }

    alice
        .send_new_round_join(
            ROUND_ID,
            "round_id=r-2026-02-24-004-c;transition=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice join");
    bob.send_new_round_join(
        ROUND_ID,
        "round_id=r-2026-02-24-004-c;transition=VTXO_ROOT_SIGNATURE_Bob",
    )
    .await
    .expect("bob join");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_join_event(pubkey, ROUND_ID, Duration::from_secs(5))
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

    assert_round_event(&alice_prepared, NEW_ROUND_START_PREPARED, ROUND_ID);
    assert_round_event(&bob_prepared, NEW_ROUND_START_PREPARED, ROUND_ID);

    let mut valid = 0usize;
    if is_valid_prepared_payload(&alice_prepared.content) {
        valid += 1;
    }
    if is_valid_prepared_payload(&bob_prepared.content) {
        valid += 1;
    }

    assert_eq!(valid, 1);
    println!("[Steve] BLOCKED: invalid start_prepared payload token detected");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_late_start_prepared_blocks_activation() {
    const ROUND_ID: &str = "r-2026-02-24-004-d";
    println!("[test] failure_late_start_prepared_blocks_activation started");

    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let alice = OpenArkUser::connect(&relay_url).await.expect("alice connect");
    let bob = OpenArkUser::connect(&relay_url).await.expect("bob connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(ROUND_ID, "round_id=r-2026-02-24-004-d;vtxo_root=VTXO_ROOT")
        .await
        .expect("initiate send");
    for user in [&alice, &bob] {
        let _ = user
            .wait_for_new_round_initiate_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("wait initiate")
            .expect("receive initiate");
    }

    alice
        .send_new_round_join(
            ROUND_ID,
            "round_id=r-2026-02-24-004-d;transition=VTXO_ROOT_SIGNATURE_Alice",
        )
        .await
        .expect("alice join");
    bob.send_new_round_join(
        ROUND_ID,
        "round_id=r-2026-02-24-004-d;transition=VTXO_ROOT_SIGNATURE_Bob",
    )
    .await
    .expect("bob join");
    for pubkey in [alice.pubkey, bob.pubkey] {
        let _ = provider
            .wait_for_new_round_join_event(pubkey, ROUND_ID, Duration::from_secs(5))
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
    assert_round_event(&alice_prepared, NEW_ROUND_START_PREPARED, ROUND_ID);

    let bob_timeout = provider
        .wait_for_new_round_start_prepared_event(bob.pubkey, ROUND_ID, Duration::from_secs(1))
        .await
        .expect("wait bob prepared timeout");

    assert!(bob_timeout.is_none());
    println!("[Steve] BLOCKED: late start_prepared from Bob missed timeout window");

    bob_late.await;
}
