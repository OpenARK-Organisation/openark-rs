use nostr_sdk::prelude::*;
use std::time::Duration;

use super::matchers::{event_has_message_type, event_has_round_id};
use super::message::{openark_round_kind, round_tags};

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
