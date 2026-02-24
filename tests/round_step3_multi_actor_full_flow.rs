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

const ROUND_ID: &str = "r-2026-02-24-003";
const USERS: [&str; 5] = ["Alice", "Bob", "Carol", "Dave", "Eve"];

struct NamedUser {
    name: &'static str,
    user: OpenArkUser,
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn step3_multi_actor_kind150_full_phase_flow() {
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

    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("[test] clients settled, executing multi-actor phase sequence");

    let mut announcement_joins = 0usize;
    let mut tree_accepts = 0usize;
    let mut prepared_signals = 0usize;
    let mut activated_receipts = 0usize;

    println!("[phase] Round Announcement");
    provider
        .send_new_round_initiate(
            ROUND_ID,
            "round_id=r-2026-02-24-003;vtxo_root=VTXO_ROOT;phase=announcement;actor=Steve",
        )
        .await
        .expect("provider should send new_round_initiate");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    for named in &users {
        let initiate = named
            .user
            .wait_for_new_round_initiate_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("user should wait for initiate")
            .expect("user should receive initiate");
        log_event(named.name, "received initiate", &initiate);
        assert_round_event(&initiate, NEW_ROUND_INITIATE);

        let join_payload = format!(
            "round_id={ROUND_ID};onboard=VTXO_ROOT;transition=VTXO_ROOT_SIGNATURE_{name};actor={name}",
            name = named.name
        );
        named
            .user
            .send_new_round_join(ROUND_ID, &join_payload)
            .await
            .expect("user should send join");
        println!("[{}] published {NEW_ROUND_JOIN}", named.name);
    }

    for named in &users {
        let join = provider
            .wait_for_new_round_join_event(named.user.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("provider should wait for join")
            .expect("provider should receive join");
        log_event("Steve", &format!("received join from {}", named.name), &join);
        assert_round_event(&join, NEW_ROUND_JOIN);
        announcement_joins += 1;
    }
    assert_eq!(announcement_joins, users.len());
    println!("[phase] Round Announcement complete: joins={announcement_joins}");

    println!("[phase] Tree Proposal");
    provider
        .send_new_round_vtxo_tree_proposal(
            ROUND_ID,
            "round_id=r-2026-02-24-003;vtxo_tree=VTXO_ROOT;forfeit_tree=FORFEIT_TREE;recycle=RECYCLE_TX;actor=Steve",
        )
        .await
        .expect("provider should send proposal");
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    for named in &users {
        let proposal = named
            .user
            .wait_for_new_round_vtxo_tree_proposal_event(
                provider.pubkey,
                ROUND_ID,
                Duration::from_secs(10),
            )
            .await
            .expect("user should wait for proposal")
            .expect("user should receive proposal");
        log_event(named.name, "received proposal", &proposal);
        assert_round_event(&proposal, NEW_ROUND_VTXO_TREE_PROPOSAL);

        let accept_payload = format!(
            "round_id={ROUND_ID};vtxo_tree_signatures=VTXO_ROOT_SIGNATURE_{name};actor={name}",
            name = named.name
        );
        named
            .user
            .send_new_round_vtxo_tree_accept(ROUND_ID, &accept_payload)
            .await
            .expect("user should send accept");
        println!("[{}] published {NEW_ROUND_VTXO_TREE_ACCEPT}", named.name);
    }

    for named in &users {
        let accept = provider
            .wait_for_new_round_vtxo_tree_accept_event(
                named.user.pubkey,
                ROUND_ID,
                Duration::from_secs(10),
            )
            .await
            .expect("provider should wait for accept")
            .expect("provider should receive accept");
        log_event("Steve", &format!("received accept from {}", named.name), &accept);
        assert_round_event(&accept, NEW_ROUND_VTXO_TREE_ACCEPT);
        tree_accepts += 1;
    }
    assert_eq!(tree_accepts, users.len());
    println!("[phase] Tree Proposal complete: accepts={tree_accepts}");

    println!("[phase] Start Preparation");
    provider
        .send_new_round_prepare_start(
            ROUND_ID,
            "round_id=r-2026-02-24-003;root_binding=VTXO_ROOT;request=FORFEIT+RECYCLE+ONBOARD_SIGS;actor=Steve",
        )
        .await
        .expect("provider should send prepare_start");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    for named in &users {
        let prepare_start = named
            .user
            .wait_for_new_round_prepare_start_event(
                provider.pubkey,
                ROUND_ID,
                Duration::from_secs(10),
            )
            .await
            .expect("user should wait for prepare_start")
            .expect("user should receive prepare_start");
        log_event(named.name, "received prepare_start", &prepare_start);
        assert_round_event(&prepare_start, NEW_ROUND_PREPARE_START);

        let prepared_payload = format!(
            "round_id={ROUND_ID};forfeit_sig=FORFEIT_SIG_{name};recycle_sig=RECYCLE_SIG_{name};root_sig=VTXO_ROOT_SIGNATURE_{name};actor={name}",
            name = named.name
        );
        named
            .user
            .send_new_round_start_prepared(ROUND_ID, &prepared_payload)
            .await
            .expect("user should send start_prepared");
        println!("[{}] published {NEW_ROUND_START_PREPARED}", named.name);
    }

    for named in &users {
        let prepared = provider
            .wait_for_new_round_start_prepared_event(
                named.user.pubkey,
                ROUND_ID,
                Duration::from_secs(10),
            )
            .await
            .expect("provider should wait for start_prepared")
            .expect("provider should receive start_prepared");
        log_event(
            "Steve",
            &format!("received start_prepared from {}", named.name),
            &prepared,
        );
        assert_round_event(&prepared, NEW_ROUND_START_PREPARED);
        prepared_signals += 1;
    }
    assert_eq!(prepared_signals, users.len());
    println!("[phase] Start Preparation complete: prepared_signals={prepared_signals}");

    println!("[phase] Round Activation");
    provider
        .send_new_round_start(
            ROUND_ID,
            "round_id=r-2026-02-24-003;round_txid=VTXO_ROOT;status=activated;actor=Steve",
        )
        .await
        .expect("provider should send round_start");
    println!("[Steve] published {NEW_ROUND_START}");

    for named in &users {
        let start = named
            .user
            .wait_for_new_round_start_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("user should wait for round_start")
            .expect("user should receive round_start");
        log_event(named.name, "received round_start", &start);
        assert_round_event(&start, NEW_ROUND_START);
        activated_receipts += 1;
    }
    assert_eq!(activated_receipts, users.len());
    println!("[phase] Round Activation complete: receipts={activated_receipts}");

    println!(
        "[test] final state: round activated with {} users (Alice, Bob, Carol, Dave, Eve)",
        users.len()
    );
    println!("[test] step3_multi_actor_kind150_full_phase_flow passed");
}
