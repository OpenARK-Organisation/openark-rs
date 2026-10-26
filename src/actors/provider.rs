use bdk_wallet::{KeychainKind, Wallet};
use nostr_sdk::prelude::*;
use std::sync::Mutex;
use std::time::Duration;

use crate::round::message::{NewRoundStartPayload, NewRoundVtxoTreeProposalPayload};
use crate::round::transport::{
    send_new_round_initiate_message, send_new_round_prepare_start_message,
    send_new_round_start_message, send_new_round_vtxo_tree_proposal_message,
    wait_for_new_round_join_message, wait_for_new_round_start_prepared_message,
    wait_for_new_round_vtxo_tree_accept_message,
};
use crate::{PROVIDER_MESSAGE, USER_REPLY_MESSAGE};

pub struct OpenArkServiceProvider {
    client: Client,
    pub pubkey: PublicKey,
    wallet: Mutex<Wallet>,
    seed_phrase: String,
}

impl OpenArkServiceProvider {
    pub async fn connect(relay_url: &str) -> Result<Self> {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let client = Client::builder().signer(keys).build();
        client.add_relay(relay_url).await?;
        client.connect().await;
        let (wallet, seed_phrase) = crate::create_regtest_bdk_wallet_with_new_seed_phrase();
        Ok(Self {
            client,
            pubkey,
            wallet: Mutex::new(wallet),
            seed_phrase,
        })
    }

    pub fn wallet_seed_phrase(&self) -> &str {
        &self.seed_phrase
    }

    pub fn wallet_receive_address(&self) -> String {
        let mut wallet = self
            .wallet
            .lock()
            .expect("wallet mutex should not be poisoned");
        wallet
            .reveal_next_address(KeychainKind::External)
            .address
            .to_string()
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
        leaf_timeout: u64,
    ) -> Result<()> {
        send_new_round_initiate_message(&self.client, closing_block, recycled_block, leaf_timeout)
            .await
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
        initiate_event_id: &EventId,
        payload: &NewRoundVtxoTreeProposalPayload,
    ) -> Result<()> {
        send_new_round_vtxo_tree_proposal_message(&self.client, initiate_event_id, payload).await
    }

    pub async fn wait_for_new_round_vtxo_tree_accept_event(
        &self,
        user_pubkey: PublicKey,
        initiate_event_id: &EventId,
        proposal_event_id: &EventId,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_new_round_vtxo_tree_accept_message(
            &self.client,
            user_pubkey,
            initiate_event_id,
            proposal_event_id,
            timeout,
        )
        .await
    }

    pub async fn send_new_round_prepare_start(
        &self,
        initiate_event_id: &EventId,
        proposal_event_id: &EventId,
    ) -> Result<()> {
        send_new_round_prepare_start_message(&self.client, initiate_event_id, proposal_event_id)
            .await
    }

    pub async fn wait_for_new_round_start_prepared_event(
        &self,
        user_pubkey: PublicKey,
        initiate_event_id: &EventId,
        prepare_start_event_id: &EventId,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_new_round_start_prepared_message(
            &self.client,
            user_pubkey,
            initiate_event_id,
            prepare_start_event_id,
            timeout,
        )
        .await
    }

    pub async fn send_new_round_start(
        &self,
        initiate_event_id: &EventId,
        payload: &NewRoundStartPayload,
    ) -> Result<()> {
        send_new_round_start_message(&self.client, initiate_event_id, payload).await
    }
}
