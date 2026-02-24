use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::round::message::{
    NEW_ROUND_PREPARE_START, NEW_ROUND_START, NEW_ROUND_START_PREPARED, NEW_ROUND_VTXO_TREE_ACCEPT,
    NEW_ROUND_VTXO_TREE_PROPOSAL,
};
use crate::round::transport::{
    send_new_round_initiate_message, send_round_message, wait_for_new_round_join_message,
    wait_for_round_message,
};
use crate::{PROVIDER_MESSAGE, USER_REPLY_MESSAGE};

pub struct OpenArkServiceProvider {
    client: Client,
    pub pubkey: PublicKey,
}

impl OpenArkServiceProvider {
    pub async fn connect(relay_url: &str) -> Result<Self> {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let client = Client::builder().signer(keys).build();
        client.add_relay(relay_url).await?;
        client.connect().await;
        Ok(Self { client, pubkey })
    }

    pub async fn send_hello(&self) -> Result<()> {
        self.client
            .send_event_builder(EventBuilder::text_note(PROVIDER_MESSAGE))
            .await?;
        Ok(())
    }

    pub async fn wait_for_user_reply(
        &self,
        user_pubkey: PublicKey,
        timeout: Duration,
    ) -> Result<bool> {
        let filter = Filter::new().kind(Kind::TextNote).author(user_pubkey);
        self.client.subscribe(filter).await?;

        let received = tokio::time::timeout(timeout, async {
            let mut notifications = self.client.notifications();
            while let Some(notification) = notifications.next().await {
                if let ClientNotification::Event { event, .. } = notification {
                    let event = event.as_ref();
                    if event.pubkey == user_pubkey && event.content == USER_REPLY_MESSAGE {
                        return true;
                    }
                }
            }
            false
        })
        .await
        .unwrap_or(false);

        Ok(received)
    }

    pub async fn send_new_round_initiate(
        &self,
        closing_block: u64,
        recycled_block: u64,
    ) -> Result<()> {
        send_new_round_initiate_message(&self.client, closing_block, recycled_block).await
    }

    pub async fn wait_for_new_round_join_event(
        &self,
        user_pubkey: PublicKey,
        initiate_event_id: &EventId,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_new_round_join_message(&self.client, user_pubkey, initiate_event_id, timeout).await
    }

    pub async fn send_new_round_vtxo_tree_proposal(
        &self,
        round_id: &str,
        payload: &str,
    ) -> Result<()> {
        send_round_message(
            &self.client,
            NEW_ROUND_VTXO_TREE_PROPOSAL,
            round_id,
            payload,
        )
        .await
    }

    pub async fn wait_for_new_round_vtxo_tree_accept_event(
        &self,
        user_pubkey: PublicKey,
        round_id: &str,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_round_message(
            &self.client,
            user_pubkey,
            NEW_ROUND_VTXO_TREE_ACCEPT,
            round_id,
            timeout,
        )
        .await
    }

    pub async fn send_new_round_prepare_start(&self, round_id: &str, payload: &str) -> Result<()> {
        send_round_message(&self.client, NEW_ROUND_PREPARE_START, round_id, payload).await
    }

    pub async fn wait_for_new_round_start_prepared_event(
        &self,
        user_pubkey: PublicKey,
        round_id: &str,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_round_message(
            &self.client,
            user_pubkey,
            NEW_ROUND_START_PREPARED,
            round_id,
            timeout,
        )
        .await
    }

    pub async fn send_new_round_start(&self, round_id: &str, payload: &str) -> Result<()> {
        send_round_message(&self.client, NEW_ROUND_START, round_id, payload).await
    }
}
