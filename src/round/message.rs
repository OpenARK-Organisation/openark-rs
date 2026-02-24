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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundJoinOnboard {
    pub outpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundJoinOffboard {
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundJoinKeys {
    pub user_pubkey: String,
    pub agent_pubkey: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundJoinPayload {
    pub onboard: Vec<NewRoundJoinOnboard>,
    pub offboard: Vec<NewRoundJoinOffboard>,
    pub keys: NewRoundJoinKeys,
    pub address_of_last_resort: Option<String>,
}

pub fn new_round_join_payload_json(payload: &NewRoundJoinPayload) -> String {
    let onboard: Vec<Value> = payload
        .onboard
        .iter()
        .map(|entry| serde_json::json!({ "outpoint": entry.outpoint }))
        .collect();
    let offboard: Vec<Value> = payload
        .offboard
        .iter()
        .map(|entry| serde_json::json!({ "address": entry.address }))
        .collect();
    let mut value = serde_json::json!({
        "onboard": onboard,
        "offboard": offboard,
        "keys": {
            "user_pubkey": payload.keys.user_pubkey,
            "agent_pubkey": payload.keys.agent_pubkey
        }
    });
    if let Some(address) = payload.address_of_last_resort.as_ref() {
        value["address_of_last_resort"] = Value::String(address.clone());
    }
    value.to_string()
}

pub fn parse_new_round_join_payload(content: &str) -> Option<NewRoundJoinPayload> {
    let value: Value = serde_json::from_str(content).ok()?;
    let onboard = value
        .get("onboard")?
        .as_array()?
        .iter()
        .map(|entry| {
            Some(NewRoundJoinOnboard {
                outpoint: entry.get("outpoint")?.as_str()?.to_string(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let offboard = value
        .get("offboard")?
        .as_array()?
        .iter()
        .map(|entry| {
            Some(NewRoundJoinOffboard {
                address: entry.get("address")?.as_str()?.to_string(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let keys = value.get("keys")?;
    let payload = NewRoundJoinPayload {
        onboard,
        offboard,
        keys: NewRoundJoinKeys {
            user_pubkey: keys.get("user_pubkey")?.as_str()?.to_string(),
            agent_pubkey: keys.get("agent_pubkey")?.as_str()?.to_string(),
        },
        address_of_last_resort: match value.get("address_of_last_resort") {
            None => None,
            Some(v) => Some(v.as_str()?.to_string()),
        },
    };
    Some(payload)
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

pub fn new_round_initiate_event_builder(closing_block: u64, recycled_block: u64) -> EventBuilder {
    EventBuilder::new(
        openark_round_kind(),
        new_round_initiate_payload_json(closing_block, recycled_block),
    )
    .tags(new_round_initiate_tags())
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

pub fn round_tags_with_event_ref(message_type: &str, event_id: &EventId) -> Vec<Tag> {
    vec![
        Tag::hashtag(message_type),
        Tag::custom(TagKind::custom("e"), [event_id.to_string()]),
    ]
}

pub fn new_round_join_tags(initiate_event_id: &EventId) -> Vec<Tag> {
    vec![
        Tag::hashtag(NEW_ROUND_JOIN),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::custom(TagKind::custom("e"), [initiate_event_id.to_string()]),
    ]
}

pub fn new_round_join_event_builder(initiate_event_id: &EventId, payload: &str) -> EventBuilder {
    EventBuilder::new(openark_round_kind(), payload.to_string()).tags(new_round_join_tags(initiate_event_id))
}

#[cfg(test)]
mod tests {
    use super::{
        NewRoundJoinKeys, NewRoundJoinOffboard, NewRoundJoinOnboard, NewRoundJoinPayload,
        new_round_join_payload_json, parse_new_round_join_payload,
    };

    #[test]
    fn round_join_payload_json_roundtrips() {
        let payload = NewRoundJoinPayload {
            onboard: vec![NewRoundJoinOnboard {
                outpoint: "abcd:0".to_string(),
            }],
            offboard: vec![NewRoundJoinOffboard {
                address: "bc1qexample".to_string(),
            }],
            keys: NewRoundJoinKeys {
                user_pubkey: "user-pubkey".to_string(),
                agent_pubkey: "agent-pubkey".to_string(),
            },
            address_of_last_resort: Some("bc1qlastresort".to_string()),
        };
        let json = new_round_join_payload_json(&payload);
        let parsed = parse_new_round_join_payload(&json).expect("payload should parse");
        assert_eq!(parsed, payload);
    }

    #[test]
    fn round_join_payload_parse_rejects_missing_required_fields() {
        let missing_keys = r#"{"onboard":[],"offboard":[]}"#;
        assert!(
            parse_new_round_join_payload(missing_keys).is_none(),
            "missing keys must be rejected"
        );
    }

    #[test]
    fn round_join_payload_parse_rejects_wrong_types() {
        let wrong_types = r#"{
            "onboard":"oops",
            "offboard":[],
            "keys":{"user_pubkey":"u","agent_pubkey":"a"}
        }"#;
        assert!(
            parse_new_round_join_payload(wrong_types).is_none(),
            "wrong onboard type must be rejected"
        );
    }
}
