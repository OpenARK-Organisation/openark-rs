use nostr_sdk::prelude::Event;
use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START,
    NEW_ROUND_START_PREPARED, NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL,
    OpenArkServiceProvider, OpenArkUser, event_has_message_type, event_has_round_id,
    event_is_openark_kind_150,
};
use std::time::Duration;
use testcontainers::{
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

const ROUND_ID: &str = "r-2026-02-24-002";

fn log_event(actor: &str, label: &str, event: &Event) {
    println!(
        "[{actor}] {label} id={} kind={} pubkey={} tags={:?} content={}",
        event.id, event.kind, event.pubkey, event.tags, event.content
    );
}

fn assert_round_event(event: &Event, message_type: &str) {
    assert!(event_is_openark_kind_150(event));
    assert!(event_has_message_type(event, message_type));
    assert!(event_has_round_id(event, ROUND_ID));
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn step2_two_actor_kind150_full_phase_flow() {
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
    provider
        .send_new_round_initiate(
            ROUND_ID,
            "round_id=r-2026-02-24-002;vtxo_root=VTXO_ROOT;phase=announcement;actor=Steve",
        )
        .await
        .expect("provider should send new_round_initiate");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    let initiate = user
        .wait_for_new_round_initiate_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("user should wait for initiate")
        .expect("user should receive initiate");
    log_event("Alice", "received initiate", &initiate);
    assert_round_event(&initiate, NEW_ROUND_INITIATE);
    assert!(initiate.content.contains("VTXO_ROOT"));
    observed_order.push(NEW_ROUND_INITIATE);

    user.send_new_round_join(
        ROUND_ID,
        "round_id=r-2026-02-24-002;onboard=VTXO_ROOT;transition=VTXO_ROOT_SIGNATURE;actor=Alice",
    )
    .await
    .expect("user should send join");
    println!("[Alice] published {NEW_ROUND_JOIN}");

    let join = provider
        .wait_for_new_round_join_event(user.pubkey, ROUND_ID, Duration::from_secs(10))
        .await
        .expect("provider should wait for join")
        .expect("provider should receive join");
    log_event("Steve", "received join", &join);
    assert_round_event(&join, NEW_ROUND_JOIN);
    assert!(join.content.contains("VTXO_ROOT_SIGNATURE"));
    observed_order.push(NEW_ROUND_JOIN);

    println!("[phase] Tree Proposal");
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
    log_event("Alice", "received proposal", &proposal);
    assert_round_event(&proposal, NEW_ROUND_VTXO_TREE_PROPOSAL);
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
    log_event("Steve", "received accept", &accept);
    assert_round_event(&accept, NEW_ROUND_VTXO_TREE_ACCEPT);
    assert!(accept.content.contains("VTXO_ROOT_SIGNATURE"));
    observed_order.push(NEW_ROUND_VTXO_TREE_ACCEPT);

    println!("[phase] Start Preparation");
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
    log_event("Alice", "received prepare_start", &prepare_start);
    assert_round_event(&prepare_start, NEW_ROUND_PREPARE_START);
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
    log_event("Steve", "received start_prepared", &prepared);
    assert_round_event(&prepared, NEW_ROUND_START_PREPARED);
    assert!(prepared.content.contains("FORFEIT_SIG_A"));
    assert!(prepared.content.contains("RECYCLE_SIG_A"));
    assert!(prepared.content.contains("VTXO_ROOT_SIGNATURE"));
    observed_order.push(NEW_ROUND_START_PREPARED);

    println!("[phase] Round Activation");
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
    log_event("Alice", "received round_start", &start);
    assert_round_event(&start, NEW_ROUND_START);
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
}
