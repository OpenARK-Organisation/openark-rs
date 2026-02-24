use nostr_sdk::prelude::*;

pub const OPENARK_KIND_150: u16 = 150;
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

pub fn round_tags(message_type: &str, round_id: &str) -> Vec<Tag> {
    vec![
        Tag::hashtag(message_type),
        Tag::custom(TagKind::custom("round_id"), [round_id]),
    ]
}
