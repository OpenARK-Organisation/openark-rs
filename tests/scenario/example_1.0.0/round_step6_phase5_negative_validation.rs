use std::time::Duration;

use nostr_sdk::prelude::*;
use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_START, OPENARK_KIND_150, OpenArkUser, ROUND_MESSAGE_VERSION,
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

async fn publish_valid_initiate_and_get_ref(
    user: &OpenArkUser,
    raw_provider: &Client,
    raw_provider_pubkey: PublicKey,
) -> EventId {
    let initiate = EventBuilder::new(
        kind_150(),
        r#"{"closing_block":910000,"recycled_block":910144,"leaf_timeout":10}"#,
    )
    .tags([
        Tag::hashtag(NEW_ROUND_INITIATE),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
    ]);
    raw_provider
        .send_event_builder(initiate)
        .await
        .expect("valid initiate should publish");

    user.wait_for_new_round_initiate_event(raw_provider_pubkey, Duration::from_secs(10))
        .await
        .expect("wait should succeed")
        .expect("initiate should be received")
        .id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failure_start_missing_v_tag_is_rejected() {
    let (_relay_container, relay_url) = start_relay().await;
    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    let (raw_provider, raw_provider_pubkey) = connect_raw_client(&relay_url).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    let round_ref =
        publish_valid_initiate_and_get_ref(&user, &raw_provider, raw_provider_pubkey).await;

    let missing_v =
        EventBuilder::new(kind_150(), r#"{"round_tx":"BASE64_SIGNED_ROOT_TX"}"#).tags([
            Tag::hashtag(NEW_ROUND_START),
            Tag::custom(TagKind::custom("e"), [round_ref.to_string()]),
        ]);
    raw_provider
        .send_event_builder(missing_v)
        .await
        .expect("missing-v start should publish");

    let received = user
        .wait_for_new_round_start_event(raw_provider_pubkey, &round_ref, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        received.is_none(),
        "start without version tag must be ignored"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failure_start_missing_or_wrong_e_tag_is_rejected() {
    let (_relay_container, relay_url) = start_relay().await;
    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    let (raw_provider, raw_provider_pubkey) = connect_raw_client(&relay_url).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    let round_ref =
        publish_valid_initiate_and_get_ref(&user, &raw_provider, raw_provider_pubkey).await;

    let missing_e =
        EventBuilder::new(kind_150(), r#"{"round_tx":"BASE64_SIGNED_ROOT_TX"}"#).tags([
            Tag::hashtag(NEW_ROUND_START),
            Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        ]);
    raw_provider
        .send_event_builder(missing_e)
        .await
        .expect("missing-e start should publish");

    let received_missing_e = user
        .wait_for_new_round_start_event(raw_provider_pubkey, &round_ref, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        received_missing_e.is_none(),
        "start without e tag must be ignored"
    );

    let wrong_e = EventBuilder::new(kind_150(), r#"{"round_tx":"BASE64_SIGNED_ROOT_TX"}"#).tags([
        Tag::hashtag(NEW_ROUND_START),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::custom(
            TagKind::custom("e"),
            ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
        ),
    ]);
    raw_provider
        .send_event_builder(wrong_e)
        .await
        .expect("wrong-e start should publish");

    let received_wrong_e = user
        .wait_for_new_round_start_event(raw_provider_pubkey, &round_ref, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        received_wrong_e.is_none(),
        "start with wrong e tag must be ignored"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failure_start_invalid_payload_is_rejected() {
    let (_relay_container, relay_url) = start_relay().await;
    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    let (raw_provider, raw_provider_pubkey) = connect_raw_client(&relay_url).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    let round_ref =
        publish_valid_initiate_and_get_ref(&user, &raw_provider, raw_provider_pubkey).await;

    let missing_round_tx = EventBuilder::new(kind_150(), r#"{}"#).tags([
        Tag::hashtag(NEW_ROUND_START),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::custom(TagKind::custom("e"), [round_ref.to_string()]),
    ]);
    raw_provider
        .send_event_builder(missing_round_tx)
        .await
        .expect("missing-round-tx start should publish");

    let received_missing_field = user
        .wait_for_new_round_start_event(raw_provider_pubkey, &round_ref, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        received_missing_field.is_none(),
        "start missing round_tx must be ignored"
    );

    let legacy_payload = EventBuilder::new(
        kind_150(),
        "round_id=r-legacy;round_txid=VTXO_ROOT;status=activated;actor=Steve",
    )
    .tags([
        Tag::hashtag(NEW_ROUND_START),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::custom(TagKind::custom("e"), [round_ref.to_string()]),
    ]);
    raw_provider
        .send_event_builder(legacy_payload)
        .await
        .expect("legacy start payload should publish");

    let received_legacy = user
        .wait_for_new_round_start_event(raw_provider_pubkey, &round_ref, Duration::from_secs(1))
        .await
        .expect("wait should succeed");
    assert!(
        received_legacy.is_none(),
        "legacy round_start content must be ignored"
    );
}
