use nostr_sdk::prelude::*;
use std::time::Duration;

use super::matchers::{
    event_has_event_reference, event_has_message_type, event_has_message_version, event_has_round_id,
};
use super::message::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, ROUND_MESSAGE_VERSION, new_round_initiate_payload_json,
    new_round_initiate_tags, openark_round_kind, parse_new_round_initiate_payload, round_tags,
    round_tags_with_event_ref,
};

pub async fn send_new_round_initiate_message(
    client: &Client,
    closing_block: u64,
    recycled_block: u64,
) -> Result<()> {
    let payload = new_round_initiate_payload_json(closing_block, recycled_block);
    let event = EventBuilder::new(openark_round_kind(), payload).tags(new_round_initiate_tags());
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_round_message(
    client: &Client,
    message_type: &str,
    round_id: &str,
    payload: &str,
) -> Result<()> {
    let event = EventBuilder::new(openark_round_kind(), payload.to_string())
        .tags(round_tags(message_type, round_id));
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_new_round_join_message(
    client: &Client,
    initiate_event_id: &EventId,
    payload: &str,
) -> Result<()> {
    let event = EventBuilder::new(openark_round_kind(), payload.to_string())
        .tags(round_tags_with_event_ref(NEW_ROUND_JOIN, initiate_event_id));
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn wait_for_new_round_initiate_message(
    client: &Client,
    expected_author: PublicKey,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(NEW_ROUND_INITIATE);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && event_has_message_type(event, NEW_ROUND_INITIATE)
                    && event_has_message_version(event, ROUND_MESSAGE_VERSION)
                    && parse_new_round_initiate_payload(&event.content).is_some()
                {
                    return Some(event.clone());
                }
            }
        }
        None
    })
    .await
    .unwrap_or(None);

    Ok(found)
}

pub async fn wait_for_round_message(
    client: &Client,
    expected_author: PublicKey,
    expected_message_type: &str,
    expected_round_id: &str,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(expected_message_type);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && event_has_message_type(event, expected_message_type)
                    && event_has_round_id(event, expected_round_id)
                {
                    return Some(event.clone());
                }
            }
        }
        None
    })
    .await
    .unwrap_or(None);

    Ok(found)
}

pub async fn wait_for_new_round_join_message(
    client: &Client,
    expected_author: PublicKey,
    expected_initiate_event_id: &EventId,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(NEW_ROUND_JOIN);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && event_has_message_type(event, NEW_ROUND_JOIN)
                    && event_has_event_reference(event, expected_initiate_event_id)
                {
                    return Some(event.clone());
                }
            }
        }
        None
    })
    .await
    .unwrap_or(None);

    Ok(found)
}
