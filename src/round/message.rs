use nostr_sdk::prelude::*;
use std::collections::BTreeMap;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundVtxoTreeProposalRecycle {
    pub unilateral_tx: String,
    pub cooperative_tx: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundVtxoTreeProposalPayload {
    pub root_tx: String,
    pub vtxo_tree: Vec<String>,
    pub forfeit_tree: Vec<String>,
    pub recycle: NewRoundVtxoTreeProposalRecycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundVtxoTreeAcceptPayload {
    pub vtxo_tree_signatures: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoundStartPreparedPayload {
    pub funding_signatures: BTreeMap<String, String>,
    pub forfeit_signatures: BTreeMap<String, String>,
    pub recycle_signature: String,
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
            let outpoint = entry.get("outpoint")?.as_str()?;
            if !looks_like_outpoint(outpoint) {
                return None;
            }
            Some(NewRoundJoinOnboard {
                outpoint: outpoint.to_string(),
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

pub fn new_round_vtxo_tree_proposal_payload_json(payload: &NewRoundVtxoTreeProposalPayload) -> String {
    serde_json::json!({
        "root_tx": payload.root_tx,
        "vtxo_tree": payload.vtxo_tree,
        "forfeit_tree": payload.forfeit_tree,
        "recycle": {
            "unilateral_tx": payload.recycle.unilateral_tx,
            "cooperative_tx": payload.recycle.cooperative_tx,
        }
    })
    .to_string()
}

pub fn parse_new_round_vtxo_tree_proposal_payload(
    content: &str,
) -> Option<NewRoundVtxoTreeProposalPayload> {
    let value: Value = serde_json::from_str(content).ok()?;
    let vtxo_tree = value
        .get("vtxo_tree")?
        .as_array()?
        .iter()
        .map(|entry| Some(entry.as_str()?.to_string()))
        .collect::<Option<Vec<_>>>()?;
    let forfeit_tree = value
        .get("forfeit_tree")?
        .as_array()?
        .iter()
        .map(|entry| Some(entry.as_str()?.to_string()))
        .collect::<Option<Vec<_>>>()?;
    let recycle = value.get("recycle")?;
    Some(NewRoundVtxoTreeProposalPayload {
        root_tx: value.get("root_tx")?.as_str()?.to_string(),
        vtxo_tree,
        forfeit_tree,
        recycle: NewRoundVtxoTreeProposalRecycle {
            unilateral_tx: recycle.get("unilateral_tx")?.as_str()?.to_string(),
            cooperative_tx: recycle.get("cooperative_tx")?.as_str()?.to_string(),
        },
    })
}

pub fn new_round_vtxo_tree_accept_payload_json(payload: &NewRoundVtxoTreeAcceptPayload) -> String {
    serde_json::json!({
        "vtxo_tree_signatures": payload.vtxo_tree_signatures
    })
    .to_string()
}

fn looks_like_hex_txid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn looks_like_outpoint(value: &str) -> bool {
    let mut parts = value.split(':');
    let Some(txid) = parts.next() else {
        return false;
    };
    let Some(vout) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    looks_like_hex_txid(txid) && !vout.is_empty() && vout.bytes().all(|b| b.is_ascii_digit())
}

pub fn parse_new_round_vtxo_tree_accept_payload(content: &str) -> Option<NewRoundVtxoTreeAcceptPayload> {
    let value: Value = serde_json::from_str(content).ok()?;
    let signatures_obj = value.get("vtxo_tree_signatures")?.as_object()?;
    let mut signatures: BTreeMap<String, String> = BTreeMap::new();
    for (outpoint, sig) in signatures_obj {
        if !looks_like_outpoint(outpoint) {
            return None;
        }
        let sig = sig.as_str()?.to_string();
        signatures.insert(outpoint.clone(), sig);
    }
    Some(NewRoundVtxoTreeAcceptPayload {
        vtxo_tree_signatures: signatures,
    })
}

pub fn new_round_start_prepared_payload_json(payload: &NewRoundStartPreparedPayload) -> String {
    serde_json::json!({
        "funding_signatures": payload.funding_signatures,
        "forfeit_signatures": payload.forfeit_signatures,
        "recycle_signature": payload.recycle_signature
    })
    .to_string()
}

pub fn parse_new_round_start_prepared_payload(content: &str) -> Option<NewRoundStartPreparedPayload> {
    let value: Value = serde_json::from_str(content).ok()?;
    let funding_obj = value.get("funding_signatures")?.as_object()?;
    let forfeit_obj = value.get("forfeit_signatures")?.as_object()?;
    let recycle_signature = value.get("recycle_signature")?.as_str()?.to_string();

    let mut funding_signatures: BTreeMap<String, String> = BTreeMap::new();
    for (outpoint, sig) in funding_obj {
        if !looks_like_outpoint(outpoint) {
            return None;
        }
        funding_signatures.insert(outpoint.clone(), sig.as_str()?.to_string());
    }

    let mut forfeit_signatures: BTreeMap<String, String> = BTreeMap::new();
    for (outpoint, sig) in forfeit_obj {
        if !looks_like_outpoint(outpoint) {
            return None;
        }
        forfeit_signatures.insert(outpoint.clone(), sig.as_str()?.to_string());
    }

    Some(NewRoundStartPreparedPayload {
        funding_signatures,
        forfeit_signatures,
        recycle_signature,
    })
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

pub fn new_round_vtxo_tree_proposal_tags(initiate_event_id: &EventId) -> Vec<Tag> {
    vec![
        Tag::hashtag(NEW_ROUND_VTXO_TREE_PROPOSAL),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::custom(TagKind::custom("e"), [initiate_event_id.to_string()]),
    ]
}

pub fn new_round_vtxo_tree_proposal_event_builder(
    initiate_event_id: &EventId,
    payload: &NewRoundVtxoTreeProposalPayload,
) -> EventBuilder {
    EventBuilder::new(
        openark_round_kind(),
        new_round_vtxo_tree_proposal_payload_json(payload),
    )
    .tags(new_round_vtxo_tree_proposal_tags(initiate_event_id))
}

pub fn new_round_vtxo_tree_accept_tags(
    initiate_event_id: &EventId,
    proposal_event_id: &EventId,
) -> Vec<Tag> {
    vec![
        Tag::hashtag(NEW_ROUND_VTXO_TREE_ACCEPT),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::from_standardized(TagStandard::Event {
            event_id: *initiate_event_id,
            relay_url: None,
            marker: Some(Marker::Root),
            public_key: None,
            uppercase: false,
        }),
        Tag::from_standardized(TagStandard::Event {
            event_id: *proposal_event_id,
            relay_url: None,
            marker: Some(Marker::Reply),
            public_key: None,
            uppercase: false,
        }),
    ]
}

pub fn new_round_vtxo_tree_accept_event_builder(
    initiate_event_id: &EventId,
    proposal_event_id: &EventId,
    payload: &NewRoundVtxoTreeAcceptPayload,
) -> EventBuilder {
    EventBuilder::new(
        openark_round_kind(),
        new_round_vtxo_tree_accept_payload_json(payload),
    )
    .tags(new_round_vtxo_tree_accept_tags(
        initiate_event_id,
        proposal_event_id,
    ))
}

pub fn new_round_prepare_start_tags(
    initiate_event_id: &EventId,
    proposal_event_id: &EventId,
) -> Vec<Tag> {
    vec![
        Tag::hashtag(NEW_ROUND_PREPARE_START),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::from_standardized(TagStandard::Event {
            event_id: *initiate_event_id,
            relay_url: None,
            marker: Some(Marker::Root),
            public_key: None,
            uppercase: false,
        }),
        Tag::from_standardized(TagStandard::Event {
            event_id: *proposal_event_id,
            relay_url: None,
            marker: Some(Marker::Reply),
            public_key: None,
            uppercase: false,
        }),
    ]
}

pub fn new_round_prepare_start_event_builder(
    initiate_event_id: &EventId,
    proposal_event_id: &EventId,
) -> EventBuilder {
    EventBuilder::new(openark_round_kind(), "")
        .tags(new_round_prepare_start_tags(initiate_event_id, proposal_event_id))
}

pub fn new_round_start_prepared_tags(
    initiate_event_id: &EventId,
    prepare_start_event_id: &EventId,
) -> Vec<Tag> {
    vec![
        Tag::hashtag(NEW_ROUND_START_PREPARED),
        Tag::custom(TagKind::custom("v"), [ROUND_MESSAGE_VERSION]),
        Tag::from_standardized(TagStandard::Event {
            event_id: *initiate_event_id,
            relay_url: None,
            marker: Some(Marker::Root),
            public_key: None,
            uppercase: false,
        }),
        Tag::from_standardized(TagStandard::Event {
            event_id: *prepare_start_event_id,
            relay_url: None,
            marker: Some(Marker::Reply),
            public_key: None,
            uppercase: false,
        }),
    ]
}

pub fn new_round_start_prepared_event_builder(
    initiate_event_id: &EventId,
    prepare_start_event_id: &EventId,
    payload: &NewRoundStartPreparedPayload,
) -> EventBuilder {
    EventBuilder::new(
        openark_round_kind(),
        new_round_start_prepared_payload_json(payload),
    )
    .tags(new_round_start_prepared_tags(
        initiate_event_id,
        prepare_start_event_id,
    ))
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
                outpoint: "1111111111111111111111111111111111111111111111111111111111111111:0".to_string(),
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
