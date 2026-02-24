use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, OpenArkServiceProvider, OpenArkUser, event_has_message_type,
    event_has_round_id, event_is_openark_kind_150,
};
use nostr_sdk::prelude::Event;
use std::time::Duration;
use testcontainers::{
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

const ROUND_ID: &str = "r-2026-02-24-001";

fn log_event(actor: &str, label: &str, event: &Event) {
    println!(
        "[{actor}] {label} id={} kind={} pubkey={} tags={:?} content={}",
        event.id, event.kind, event.pubkey, event.tags, event.content
    );
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
async fn step1_two_actor_kind150_round_handshake() {
    println!("[test] step1_two_actor_kind150_round_handshake started");
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
    println!("[test] clients settled, starting concurrent handshake flows");

    let user_flow = async {
        println!("[Alice] waiting for {NEW_ROUND_INITIATE} round_id={ROUND_ID}");
        let maybe_initiate = user
            .wait_for_new_round_initiate_event(provider.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("user should wait for initiate");

        let initiate = maybe_initiate.expect("user should receive new_round_initiate");
        log_event("Alice", "received initiate", &initiate);
        println!("[Alice] validating initiate assertions");
        assert!(event_is_openark_kind_150(&initiate));
        assert!(event_has_message_type(&initiate, NEW_ROUND_INITIATE));
        assert!(event_has_round_id(&initiate, ROUND_ID));
        assert!(initiate.content.contains("VTXO_ROOT"));

        println!("[Alice] publishing {NEW_ROUND_JOIN} round_id={ROUND_ID}");
        user.send_new_round_join(
            ROUND_ID,
            "round_id=r-2026-02-24-001;transition=VTXO_ROOT_SIGNATURE;actor=Alice",
        )
        .await
        .expect("user should send new_round_join");
        println!("[Alice] published {NEW_ROUND_JOIN}");

        true
    };

    let provider_flow = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        println!("[Steve] publishing {NEW_ROUND_INITIATE} round_id={ROUND_ID}");
        provider
            .send_new_round_initiate(
                ROUND_ID,
                "round_id=r-2026-02-24-001;vtxo_root=VTXO_ROOT;actor=Steve",
            )
            .await
            .expect("provider should send new_round_initiate");
        println!("[Steve] published {NEW_ROUND_INITIATE}");
        println!("[Steve] waiting for {NEW_ROUND_JOIN} from Alice");

        let maybe_join = provider
            .wait_for_new_round_join_event(user.pubkey, ROUND_ID, Duration::from_secs(10))
            .await
            .expect("provider should wait for new_round_join");

        let join = maybe_join.expect("provider should receive new_round_join");
        log_event("Steve", "received join", &join);
        println!("[Steve] validating join assertions");
        assert!(event_is_openark_kind_150(&join));
        assert!(event_has_message_type(&join, NEW_ROUND_JOIN));
        assert!(event_has_round_id(&join, ROUND_ID));
        assert!(join.content.contains("VTXO_ROOT_SIGNATURE"));

        true
    };

    let (user_done, provider_done) = tokio::join!(user_flow, provider_flow);
    assert!(user_done);
    assert!(provider_done);
    println!("[test] step1_two_actor_kind150_round_handshake passed");
}
