use nostr_sdk::prelude::*;
use std::str::FromStr;
use std::time::Duration;

pub const PROVIDER_MESSAGE: &str = "hello";
pub const USER_REPLY_MESSAGE: &str = "how are you today";

pub const OPENARK_KIND_150: u16 = 150;
pub const NEW_ROUND_INITIATE: &str = "new_round_initiate";
pub const NEW_ROUND_JOIN: &str = "new_round_join";

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

    pub async fn send_new_round_initiate(&self, round_id: &str, payload: &str) -> Result<()> {
        send_round_message(&self.client, NEW_ROUND_INITIATE, round_id, payload).await
    }

    pub async fn wait_for_new_round_join_event(
        &self,
        user_pubkey: PublicKey,
        round_id: &str,
        timeout: Duration,
    ) -> Result<Option<Event>> {
        wait_for_round_message(
            &self.client,
            user_pubkey,
            NEW_ROUND_JOIN,
            round_id,
            timeout,
        )
        .await
    }
}

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
}

async fn send_round_message(
    client: &Client,
    message_type: &str,
    round_id: &str,
    payload: &str,
) -> Result<()> {
    let event = EventBuilder::new(openark_round_kind(), payload.to_string()).tags(round_tags(
        message_type,
        round_id,
    ));
    client.send_event_builder(event).await?;
    Ok(())
}

async fn wait_for_round_message(
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

fn round_tags(message_type: &str, round_id: &str) -> Vec<Tag> {
    vec![
        Tag::hashtag(message_type),
        Tag::custom(TagKind::custom("round_id"), [round_id]),
    ]
}

fn openark_round_kind() -> Kind {
    Kind::Custom(OPENARK_KIND_150)
}

pub fn event_is_openark_kind_150(event: &Event) -> bool {
    event.kind == openark_round_kind()
}

pub fn event_has_message_type(event: &Event, expected_message_type: &str) -> bool {
    event
        .tags
        .hashtags()
        .any(|tag| tag.eq_ignore_ascii_case(expected_message_type))
}

pub fn event_has_round_id(event: &Event, expected_round_id: &str) -> bool {
    event_round_id(event)
        .map(|v| v == expected_round_id)
        .unwrap_or(false)
}

pub fn event_round_id(event: &Event) -> Option<String> {
    event
        .tags
        .find(TagKind::custom("round_id"))
        .and_then(|tag| tag.content())
        .map(ToOwned::to_owned)
}

pub fn create_regtest_bdk_wallet() -> bdk_wallet::Wallet {
    use bdk_wallet::bitcoin::{Network, bip32::Xpriv};
    use bdk_wallet::template::Bip84;
    use bdk_wallet::{KeychainKind, Wallet};

    let xprv = Xpriv::from_str("tprv8ZgxMBicQKsPeZRHk4rTG6orPS2CRNFX3njhUXx5vj9qGog5ZMH4uGReDWN5kCkY3jmWEtWause41CDvBRXD1shKknAMKxT99o9qUTRVC6m")
        .expect("hardcoded test xprv should parse");

    Wallet::create(
        Bip84(xprv.clone(), KeychainKind::External),
        Bip84(xprv, KeychainKind::Internal),
    )
    .network(Network::Regtest)
    .create_wallet_no_persist()
    .expect("BDK wallet descriptor should be valid")
}
