use nostr_sdk::prelude::*;
use serde_json::Value;

pub const OPENARK_KIND_150: u16 = 150;
pub const ROUND_MESSAGE_VERSION: &str = "1.0.0";
pub const NEW_ROUND_INITIATE: &str = "new_round_initiate";
pub const NEW_ROUND_JOIN: &str = "new_round_join";
pub const NEW_ROUND_VTXO_TREE_PROPOSAL: &str = "new_round_vtxo_tree_proposal";
pub const NEW_ROUND_VTXO_TREE_ACCEPT: &str = "new_round_vtxo_tree_accept";
pub const NEW_ROUND_PREPARE_START: &str = "new_round_prepare_start";
pub const NEW_ROUND_START_PREPARED: &str = "new_round_start_prepared";
pub const NEW_ROUND_START: &str = "new_round_start";

pub fn openark_round_kind() -> Kind {
    Kind::Custom(OPENARK_KIND_150)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundInitiatePayload {
    pub closing_block: u64,
    pub recycled_block: u64,
}

pub fn new_round_initiate_tags() -> Vec<Tag> {
    vec![
        Tag::hashtag(NEW_ROUND_INITIATE),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
    ]
}

pub fn new_round_initiate_payload_json(closing_block: u64, recycled_block: u64) -> String {
    format!(r#"{{"closing_block":{closing_block},"recycled_block":{recycled_block}}}"#)
}

pub fn parse_new_round_initiate_payload(content: &str) -> Option<NewRoundInitiatePayload> {
    let value: Value = serde_json::from_str(content).ok()?;
    Some(NewRoundInitiatePayload {
        closing_block: value.get("closing_block")?.as_u64()?,
        recycled_block: value.get("recycled_block")?.as_u64()?,
    })
}

pub fn round_tags(message_type: &str, round_id: &str) -> Vec<Tag> {
    vec![
        Tag::hashtag(message_type),
        Tag::custom(TagKind::custom("round_id"), [round_id]),
    ]
}
