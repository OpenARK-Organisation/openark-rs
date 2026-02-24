use std::time::Duration;

use nostr_sdk::prelude::*;
use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, OPENARK_KIND_150, OpenArkServiceProvider, OpenArkUser,
    ROUND_MESSAGE_VERSION,
};
mod support;
use support::start_relay;

async fn connect_raw_client(relay_url: &str) -> (Client, PublicKey) {
    let keys = Keys::generate();
    let pubkey = keys.public_key();
    let client = Client::builder().signer(keys).build();
    client
        .add_relay(relay_url)
        .await
        .expect("raw client should add relay");
    client.connect().await;
    (client, pubkey)
}

fn kind_150() -> Kind {
    Kind::Custom(OPENARK_KIND_150)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failure_initiate_missing_v_tag_is_rejected() {
    let (_relay_container, relay_url) = start_relay().await;
    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    let (raw_provider, raw_provider_pubkey) = connect_raw_client(&relay_url).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Missing required ["v","1.0.0"] tag.
    let malformed = EventBuilder::new(
        kind_150(),
        r#"{"closing_block":910000,"recycled_block":910144}"#,
    )
    .tags([Tag::hashtag(NEW_ROUND_INITIATE)]);
    raw_provider
        .send_event_builder(malformed)
        .await
        .expect("malformed initiate should be published");

    let received = user
        .wait_for_new_round_initiate_event(raw_provider_pubkey, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        received.is_none(),
        "initiate without version tag must be ignored"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failure_initiate_missing_required_payload_field_is_rejected() {
    let (_relay_container, relay_url) = start_relay().await;
    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    let (raw_provider, raw_provider_pubkey) = connect_raw_client(&relay_url).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Missing recycled_block field.
    let malformed = EventBuilder::new(kind_150(), r#"{"closing_block":910000}"#).tags([
        Tag::hashtag(NEW_ROUND_INITIATE),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
    ]);
    raw_provider
        .send_event_builder(malformed)
        .await
        .expect("malformed initiate should be published");

    let received = user
        .wait_for_new_round_initiate_event(raw_provider_pubkey, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        received.is_none(),
        "initiate missing required payload fields must be ignored"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failure_join_missing_or_wrong_e_tag_is_rejected() {
    let (_relay_container, relay_url) = start_relay().await;
    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    let (raw_user, raw_user_pubkey) = connect_raw_client(&relay_url).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    provider
        .send_new_round_initiate(910000, 910144)
        .await
        .expect("provider should publish initiate");
    let initiate = user
        .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
        .await
        .expect("wait should succeed")
        .expect("initiate should be received");

    let expected_round_ref = initiate.id;

    // Missing e tag.
    let missing_e = EventBuilder::new(kind_150(), "join-without-e").tags([Tag::hashtag(NEW_ROUND_JOIN)]);
    raw_user
        .send_event_builder(missing_e)
        .await
        .expect("missing-e join should publish");

    let missing_e_received = provider
        .wait_for_new_round_join_event(raw_user_pubkey, &expected_round_ref, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        missing_e_received.is_none(),
        "join without e tag must be ignored"
    );

    // Wrong e tag.
    let wrong_e = EventBuilder::new(kind_150(), "join-with-wrong-e").tags([
        Tag::hashtag(NEW_ROUND_JOIN),
        Tag::custom(
            TagKind::custom("e"),
            ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
        ),
    ]);
    raw_user
        .send_event_builder(wrong_e)
        .await
        .expect("wrong-e join should publish");

    let wrong_e_received = provider
        .wait_for_new_round_join_event(raw_user_pubkey, &expected_round_ref, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        wrong_e_received.is_none(),
        "join with wrong e tag must be ignored"
    );
}
