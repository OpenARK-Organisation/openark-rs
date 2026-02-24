use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, OpenArkServiceProvider, OpenArkUser, event_event_reference,
    event_has_message_type, event_has_event_reference, event_has_message_version,
    event_is_openark_kind_150,
    parse_new_round_initiate_payload, validate_new_round_initiate_event,
    validate_new_round_join_event_reference, ROUND_MESSAGE_VERSION,
};
use std::time::Duration;
mod support;
use support::{TraceRecorder, start_relay};

const ROUND_ID: &str = "r-2026-02-24-001";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn step1_two_actor_kind150_round_handshake() {
    let mut trace = TraceRecorder::new("round_step1_nostr_kind150", Some(ROUND_ID));
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
    println!("[test] clients settled, starting handshake flow");
    println!("[Steve] publishing {NEW_ROUND_INITIATE} round_id={ROUND_ID}");
    provider
        .send_new_round_initiate(910000, 910144)
        .await
        .expect("provider should send new_round_initiate");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    println!("[Alice] waiting for {NEW_ROUND_INITIATE} round_id={ROUND_ID}");
    let initiate = user
        .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
        .await
        .expect("user should wait for initiate")
        .expect("user should receive new_round_initiate");
    trace.event("Alice", "received initiate", &initiate);
    println!("[Alice] validating initiate assertions");
    assert!(event_is_openark_kind_150(&initiate));
    assert!(event_has_message_type(&initiate, NEW_ROUND_INITIATE));
    assert!(event_has_message_version(&initiate, ROUND_MESSAGE_VERSION));
    assert!(
        validate_new_round_initiate_event(&initiate).is_ok(),
        "initiate event should pass strict validator"
    );
    let initiate_payload = parse_new_round_initiate_payload(&initiate.content)
        .expect("initiate payload should parse");
    assert_eq!(initiate_payload.closing_block, 910000);
    assert_eq!(initiate_payload.recycled_block, 910144);
    let round_ref = initiate.id;
    println!("[round] reference={round_ref}");

    println!("[Alice] publishing {NEW_ROUND_JOIN} round_id={ROUND_ID}");
    user.send_new_round_join(
        &round_ref,
        "round_id=r-2026-02-24-001;transition=VTXO_ROOT_SIGNATURE;actor=Alice",
    )
    .await
    .expect("user should send new_round_join");
    println!("[Alice] published {NEW_ROUND_JOIN}");

    println!("[Steve] waiting for {NEW_ROUND_JOIN} from Alice");
    let join = provider
        .wait_for_new_round_join_event(user.pubkey, &round_ref, Duration::from_secs(10))
        .await
        .expect("provider should wait for new_round_join")
        .expect("provider should receive new_round_join");
    trace.event("Steve", "received join", &join);
    println!("[Steve] validating join assertions");
    assert!(event_is_openark_kind_150(&join));
    assert!(event_has_message_type(&join, NEW_ROUND_JOIN));
    assert!(event_has_event_reference(&join, &round_ref));
    assert!(
        validate_new_round_join_event_reference(&join, &round_ref).is_ok(),
        "join event should reference initiate id via e tag"
    );
    println!(
        "[round] join_event_reference={}",
        event_event_reference(&join).expect("join should have e reference")
    );
    assert!(join.content.contains("VTXO_ROOT_SIGNATURE"));
    println!("[test] step1_two_actor_kind150_round_handshake passed");
    trace.finish(
        "passed",
        serde_json::json!({
            "initiate_received": true,
            "join_received": true
        }),
    );
}
