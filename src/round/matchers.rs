use nostr_sdk::prelude::*;
use serde_json::Value;

use super::message::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START_PREPARED,
    NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL, ROUND_MESSAGE_VERSION,
    openark_round_kind,
    parse_new_round_initiate_payload, parse_new_round_join_payload,
    parse_new_round_prepare_start_payload, parse_new_round_start_prepared_payload,
    parse_new_round_vtxo_tree_accept_payload, parse_new_round_vtxo_tree_proposal_payload,
};

pub fn event_is_openark_kind_150(event: &Event) -> bool {
    event.kind == openark_round_kind()
}

pub fn event_has_message_type(event: &Event, expected_message_type: &str) -> bool {
    event
        .tags
        .hashtags()
        .any(|tag| tag.eq_ignore_ascii_case(expected_message_type))
}

pub fn event_has_message_version(event: &Event, expected_version: &str) -> bool {
    event
        .tags
        .find(TagKind::custom("v"))
        .and_then(|tag| tag.content())
        .map(|v| v == expected_version)
        .unwrap_or(false)
}

pub fn event_has_event_reference(event: &Event, expected_event_id: &EventId) -> bool {
    event_event_reference(event)
        .map(|v| v == expected_event_id.to_string())
        .unwrap_or(false)
}

pub fn event_has_round_id(event: &Event, expected_round_id: &str) -> bool {
    event_round_id(event)
        .map(|v| v == expected_round_id)
        .unwrap_or(false)
}

pub fn event_event_reference(event: &Event) -> Option<String> {
    event
        .tags
        .find(TagKind::custom("e"))
        .and_then(|tag| tag.content())
        .map(ToOwned::to_owned)
}

pub fn event_event_reference_by_marker(event: &Event, expected_marker: Marker) -> Option<EventId> {
    for tag in event.tags.iter() {
        let Some(standard) = tag.as_standardized() else {
            continue;
        };
        if let TagStandard::Event {
            event_id,
            marker: Some(marker),
            ..
        } = standard
            && *marker == expected_marker
        {
            return Some(*event_id);
        }
    }
    None
}

pub fn validate_new_round_initiate_event(event: &Event) -> Result<(), &'static str> {
    if !event_is_openark_kind_150(event) {
        return Err("wrong_kind");
    }
    if !event_has_message_type(event, NEW_ROUND_INITIATE) {
        return Err("missing_or_wrong_t");
    }
    if !event_has_message_version(event, ROUND_MESSAGE_VERSION) {
        return Err("missing_or_wrong_v");
    }
    if parse_new_round_initiate_payload(&event.content).is_none() {
        return Err("invalid_initiate_payload");
    }
    Ok(())
}

pub fn validate_new_round_join_event_reference(
    event: &Event,
    expected_initiate_event_id: &EventId,
) -> Result<(), &'static str> {
    if !event_is_openark_kind_150(event) {
        return Err("wrong_kind");
    }
    if !event_has_message_type(event, NEW_ROUND_JOIN) {
        return Err("missing_or_wrong_t");
    }
    if !event_has_message_version(event, ROUND_MESSAGE_VERSION) {
        return Err("missing_or_wrong_v");
    }
    if !event_has_event_reference(event, expected_initiate_event_id) {
        return Err("missing_or_wrong_e");
    }
    let parsed: Value = serde_json::from_str(&event.content).map_err(|_| "invalid_join_payload")?;
    if parsed.get("round_id").is_some() {
        return Err("forbidden_round_id_in_content");
    }
    if parse_new_round_join_payload(&event.content).is_none() {
        return Err("invalid_join_payload");
    }
    Ok(())
}

pub fn validate_new_round_vtxo_tree_proposal_event(
    event: &Event,
    expected_initiate_event_id: &EventId,
) -> Result<(), &'static str> {
    if !event_is_openark_kind_150(event) {
        return Err("wrong_kind");
    }
    if !event_has_message_type(event, NEW_ROUND_VTXO_TREE_PROPOSAL) {
        return Err("missing_or_wrong_t");
    }
    if !event_has_message_version(event, ROUND_MESSAGE_VERSION) {
        return Err("missing_or_wrong_v");
    }
    if !event_has_event_reference(event, expected_initiate_event_id) {
        return Err("missing_or_wrong_e");
    }
    if parse_new_round_vtxo_tree_proposal_payload(&event.content).is_none() {
        return Err("invalid_vtxo_tree_proposal_payload");
    }
    Ok(())
}

pub fn validate_new_round_vtxo_tree_accept_event(
    event: &Event,
    expected_initiate_event_id: &EventId,
    expected_proposal_event_id: &EventId,
) -> Result<(), &'static str> {
    if !event_is_openark_kind_150(event) {
        return Err("wrong_kind");
    }
    if !event_has_message_type(event, NEW_ROUND_VTXO_TREE_ACCEPT) {
        return Err("missing_or_wrong_t");
    }
    if !event_has_message_version(event, ROUND_MESSAGE_VERSION) {
        return Err("missing_or_wrong_v");
    }
    if event_event_reference_by_marker(event, Marker::Root) != Some(*expected_initiate_event_id) {
        return Err("missing_or_wrong_root_reference");
    }
    if event_event_reference_by_marker(event, Marker::Reply) != Some(*expected_proposal_event_id) {
        return Err("missing_or_wrong_reply_reference");
    }
    let parsed: Value =
        serde_json::from_str(&event.content).map_err(|_| "invalid_vtxo_tree_accept_payload")?;
    if !parsed.get("vtxo_tree_signatures").is_some_and(|v| v.is_object()) {
        return Err("invalid_vtxo_tree_accept_payload");
    }
    if parse_new_round_vtxo_tree_accept_payload(&event.content).is_none() {
        return Err("invalid_vtxo_tree_accept_payload");
    }
    Ok(())
}

pub fn validate_new_round_prepare_start_event(
    event: &Event,
    expected_initiate_event_id: &EventId,
) -> Result<(), &'static str> {
    if !event_is_openark_kind_150(event) {
        return Err("wrong_kind");
    }
    if !event_has_message_type(event, NEW_ROUND_PREPARE_START) {
        return Err("missing_or_wrong_t");
    }
    if !event_has_message_version(event, ROUND_MESSAGE_VERSION) {
        return Err("missing_or_wrong_v");
    }
    if !event_has_event_reference(event, expected_initiate_event_id) {
        return Err("missing_or_wrong_e");
    }
    if parse_new_round_prepare_start_payload(&event.content).is_none() {
        return Err("invalid_prepare_start_payload");
    }
    Ok(())
}

pub fn validate_new_round_start_prepared_event(
    event: &Event,
    expected_initiate_event_id: &EventId,
    expected_prepare_start_event_id: &EventId,
) -> Result<(), &'static str> {
    if !event_is_openark_kind_150(event) {
        return Err("wrong_kind");
    }
    if !event_has_message_type(event, NEW_ROUND_START_PREPARED) {
        return Err("missing_or_wrong_t");
    }
    if !event_has_message_version(event, ROUND_MESSAGE_VERSION) {
        return Err("missing_or_wrong_v");
    }
    if event_event_reference_by_marker(event, Marker::Root) != Some(*expected_initiate_event_id) {
        return Err("missing_or_wrong_root_reference");
    }
    if event_event_reference_by_marker(event, Marker::Reply) != Some(*expected_prepare_start_event_id) {
        return Err("missing_or_wrong_reply_reference");
    }
    if parse_new_round_start_prepared_payload(&event.content).is_none() {
        return Err("invalid_start_prepared_payload");
    }
    Ok(())
}

pub fn event_round_id(event: &Event) -> Option<String> {
    event
        .tags
        .find(TagKind::custom("round_id"))
        .and_then(|tag| tag.content())
        .map(ToOwned::to_owned)
}
