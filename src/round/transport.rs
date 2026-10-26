use nostr_sdk::prelude::*;
use std::time::Duration;

use super::matchers::{
    event_has_message_type, event_has_round_id, validate_new_round_initiate_event,
    validate_new_round_join_event_reference, validate_new_round_prepare_start_event,
    validate_new_round_start_event, validate_new_round_start_prepared_event,
    validate_new_round_vtxo_tree_accept_event, validate_new_round_vtxo_tree_proposal_event,
};
use super::message::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START,
    NEW_ROUND_START_PREPARED, NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL,
    NewRoundJoinPayload, NewRoundStartPayload, NewRoundStartPreparedPayload,
    NewRoundVtxoTreeAcceptPayload, NewRoundVtxoTreeProposalPayload,
    new_round_initiate_event_builder, new_round_join_event_builder, new_round_join_payload_json,
    new_round_prepare_start_event_builder, new_round_start_event_builder,
    new_round_start_prepared_event_builder, new_round_vtxo_tree_accept_event_builder,
    new_round_vtxo_tree_proposal_event_builder, openark_round_kind, round_tags,
};

pub async fn send_new_round_initiate_message(
    client: &Client,
    closing_block: u64,
    recycled_block: u64,
    leaf_timeout: u64,
) -> Result<()> {
    let event = new_round_initiate_event_builder(closing_block, recycled_block, leaf_timeout);
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_round_message(
    client: &Client,
    message_type: &str,
    round_id: &str,
    payload: &str,
) -> Result<()> {
    assert_ne!(
        message_type, NEW_ROUND_INITIATE,
        "new_round_initiate must use canonical initiate builder"
    );
    assert_ne!(
        message_type, NEW_ROUND_JOIN,
        "new_round_join must use canonical join builder"
    );
    assert_ne!(
        message_type, NEW_ROUND_VTXO_TREE_PROPOSAL,
        "new_round_vtxo_tree_proposal must use canonical proposal builder"
    );
    assert_ne!(
        message_type, NEW_ROUND_VTXO_TREE_ACCEPT,
        "new_round_vtxo_tree_accept must use canonical accept builder"
    );
    assert_ne!(
        message_type, NEW_ROUND_PREPARE_START,
        "new_round_prepare_start must use canonical prepare_start builder"
    );
    assert_ne!(
        message_type, NEW_ROUND_START_PREPARED,
        "new_round_start_prepared must use canonical start_prepared builder"
    );
    assert_ne!(
        message_type, NEW_ROUND_START,
        "new_round_start must use canonical start builder"
    );
    let event = EventBuilder::new(openark_round_kind(), payload.to_string())
        .tags(round_tags(message_type, round_id));
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_new_round_join_message(
    client: &Client,
    initiate_event_id: &EventId,
    payload: &NewRoundJoinPayload,
) -> Result<()> {
    let payload_json = new_round_join_payload_json(payload);
    let event = new_round_join_event_builder(initiate_event_id, &payload_json);
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_new_round_vtxo_tree_proposal_message(
    client: &Client,
    initiate_event_id: &EventId,
    payload: &NewRoundVtxoTreeProposalPayload,
) -> Result<()> {
    let event = new_round_vtxo_tree_proposal_event_builder(initiate_event_id, payload);
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_new_round_vtxo_tree_accept_message(
    client: &Client,
    initiate_event_id: &EventId,
    proposal_event_id: &EventId,
    payload: &NewRoundVtxoTreeAcceptPayload,
) -> Result<()> {
    let event =
        new_round_vtxo_tree_accept_event_builder(initiate_event_id, proposal_event_id, payload);
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_new_round_prepare_start_message(
    client: &Client,
    initiate_event_id: &EventId,
    proposal_event_id: &EventId,
) -> Result<()> {
    let event = new_round_prepare_start_event_builder(initiate_event_id, proposal_event_id);
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_new_round_start_prepared_message(
    client: &Client,
    initiate_event_id: &EventId,
    prepare_start_event_id: &EventId,
    payload: &NewRoundStartPreparedPayload,
) -> Result<()> {
    let event =
        new_round_start_prepared_event_builder(initiate_event_id, prepare_start_event_id, payload);
    client.send_event_builder(event).await?;
    Ok(())
}

pub async fn send_new_round_start_message(
    client: &Client,
    initiate_event_id: &EventId,
    payload: &NewRoundStartPayload,
) -> Result<()> {
    let event = new_round_start_event_builder(initiate_event_id, payload);
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
                    && validate_new_round_initiate_event(event).is_ok()
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
    assert_ne!(
        expected_message_type, NEW_ROUND_INITIATE,
        "new_round_initiate must use canonical initiate wait path"
    );
    assert_ne!(
        expected_message_type, NEW_ROUND_JOIN,
        "new_round_join must use canonical join wait path"
    );
    assert_ne!(
        expected_message_type, NEW_ROUND_VTXO_TREE_PROPOSAL,
        "new_round_vtxo_tree_proposal must use canonical proposal wait path"
    );
    assert_ne!(
        expected_message_type, NEW_ROUND_VTXO_TREE_ACCEPT,
        "new_round_vtxo_tree_accept must use canonical accept wait path"
    );
    assert_ne!(
        expected_message_type, NEW_ROUND_PREPARE_START,
        "new_round_prepare_start must use canonical prepare_start wait path"
    );
    assert_ne!(
        expected_message_type, NEW_ROUND_START_PREPARED,
        "new_round_start_prepared must use canonical start_prepared wait path"
    );
    assert_ne!(
        expected_message_type, NEW_ROUND_START,
        "new_round_start must use canonical start wait path"
    );
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
                    && validate_new_round_join_event_reference(event, expected_initiate_event_id)
                        .is_ok()
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

pub async fn wait_for_new_round_vtxo_tree_proposal_message(
    client: &Client,
    expected_author: PublicKey,
    expected_initiate_event_id: &EventId,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(NEW_ROUND_VTXO_TREE_PROPOSAL);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && validate_new_round_vtxo_tree_proposal_event(
                        event,
                        expected_initiate_event_id,
                    )
                    .is_ok()
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

pub async fn wait_for_new_round_vtxo_tree_accept_message(
    client: &Client,
    expected_author: PublicKey,
    expected_initiate_event_id: &EventId,
    expected_proposal_event_id: &EventId,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(NEW_ROUND_VTXO_TREE_ACCEPT);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && validate_new_round_vtxo_tree_accept_event(
                        event,
                        expected_initiate_event_id,
                        expected_proposal_event_id,
                    )
                    .is_ok()
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

pub async fn wait_for_new_round_prepare_start_message(
    client: &Client,
    expected_author: PublicKey,
    expected_initiate_event_id: &EventId,
    expected_proposal_event_id: &EventId,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(NEW_ROUND_PREPARE_START);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && validate_new_round_prepare_start_event(
                        event,
                        expected_initiate_event_id,
                        expected_proposal_event_id,
                    )
                    .is_ok()
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

pub async fn wait_for_new_round_start_prepared_message(
    client: &Client,
    expected_author: PublicKey,
    expected_initiate_event_id: &EventId,
    expected_prepare_start_event_id: &EventId,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(NEW_ROUND_START_PREPARED);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && validate_new_round_start_prepared_event(
                        event,
                        expected_initiate_event_id,
                        expected_prepare_start_event_id,
                    )
                    .is_ok()
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
pub async fn wait_for_new_round_start_message(
    client: &Client,
    expected_author: PublicKey,
    expected_initiate_event_id: &EventId,
    timeout: Duration,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .kind(openark_round_kind())
        .author(expected_author)
        .hashtag(NEW_ROUND_START);
    client.subscribe(filter).await?;

    let found = tokio::time::timeout(timeout, async {
        let mut notifications = client.notifications();
        while let Some(notification) = notifications.next().await {
            if let ClientNotification::Event { event, .. } = notification {
                let event = event.as_ref();
                if event.pubkey == expected_author
                    && validate_new_round_start_event(event, expected_initiate_event_id).is_ok()
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
