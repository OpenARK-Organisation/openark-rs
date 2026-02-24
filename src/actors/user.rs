use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::round::message::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START,
    NEW_ROUND_START_PREPARED, NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL,
};
use crate::round::transport::{send_round_message, wait_for_round_message};
use crate::{PROVIDER_MESSAGE, USER_REPLY_MESSAGE};

pub struct OpenArkUser {
    client: Client,
    pub pubkey: PublicKey,
}

impl OpenArkUser {
    pub async fn connect(relay_url: &str) -> Result<Self> {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let client = Client::builder().signer(keys).build();
        client.add_relay(relay_url).await?;
        client.connect().await;
        Ok(Self { client, pubkey })
    }

    pub async fn reply_when_provider_says_hello(
        &self,
        provider_pubkey: PublicKey,
        timeout: Duration,
    ) -> Result<bool> {
        let filter = Filter::new().kind(Kind::TextNote).author(provider_pubkey);
        self.client.subscribe(filter).await?;

        let should_reply = tokio::time::timeout(timeout, async {
            let mut notifications = self.client.notifications();
            while let Some(notification) = notifications.next().await {
                if let ClientNotification::Event { event, .. } = notification {
                    let event = event.as_ref();
                    if event.pubkey == provider_pubkey && event.content == PROVIDER_MESSAGE {
                        return true;
                    }
                }
            }
            false
        })
        .await
        .unwrap_or(false);

        if should_reply {
            self.client
                .send_event_builder(EventBuilder::text_note(USER_REPLY_MESSAGE))
                .await?;
        }

        Ok(should_reply)
    }

    pub async fn wait_for_new_round_initiate_event(
        &self,
        provider_pubkey: PublicKey,
        round_id: &str,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_round_message(
            &self.client,
            provider_pubkey,
            NEW_ROUND_INITIATE,
            round_id,
            timeout,
        )
        .await
    }

    pub async fn send_new_round_join(&self, round_id: &str, payload: &str) -> Result<()> {
        send_round_message(&self.client, NEW_ROUND_JOIN, round_id, payload).await
    }

    pub async fn wait_for_new_round_vtxo_tree_proposal_event(
        &self,
        provider_pubkey: PublicKey,
        round_id: &str,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_round_message(
            &self.client,
            provider_pubkey,
            NEW_ROUND_VTXO_TREE_PROPOSAL,
            round_id,
            timeout,
        )
        .await
    }

    pub async fn send_new_round_vtxo_tree_accept(&self, round_id: &str, payload: &str) -> Result<()> {
        send_round_message(
            &self.client,
            NEW_ROUND_VTXO_TREE_ACCEPT,
            round_id,
            payload,
        )
        .await
    }

    pub async fn wait_for_new_round_prepare_start_event(
        &self,
        provider_pubkey: PublicKey,
        round_id: &str,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_round_message(
            &self.client,
            provider_pubkey,
            NEW_ROUND_PREPARE_START,
            round_id,
            timeout,
        )
        .await
    }

    pub async fn send_new_round_start_prepared(&self, round_id: &str, payload: &str) -> Result<()> {
        send_round_message(
            &self.client,
            NEW_ROUND_START_PREPARED,
            round_id,
            payload,
        )
        .await
    }

    pub async fn wait_for_new_round_start_event(
        &self,
        provider_pubkey: PublicKey,
        round_id: &str,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_round_message(
            &self.client,
            provider_pubkey,
            NEW_ROUND_START,
            round_id,
            timeout,
        )
        .await
    }
}
