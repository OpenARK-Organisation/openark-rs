use nostr_sdk::prelude::*;

use super::message::openark_round_kind;

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
