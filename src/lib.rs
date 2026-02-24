pub mod actors;
pub mod round;
pub mod wallet;

pub use actors::{OpenArkServiceProvider, OpenArkUser};
pub use round::{
    NewRoundJoinKeys,
    NewRoundJoinOffboard,
    NewRoundJoinOnboard,
    NewRoundJoinPayload,
    NEW_ROUND_INITIATE,
    NEW_ROUND_JOIN,
    NEW_ROUND_PREPARE_START,
    NEW_ROUND_START,
    NEW_ROUND_START_PREPARED,
    NEW_ROUND_VTXO_TREE_ACCEPT,
    NEW_ROUND_VTXO_TREE_PROPOSAL,
    ROUND_MESSAGE_VERSION,
    OPENARK_KIND_150,
    event_event_reference,
    event_has_event_reference,
    event_has_message_type,
    event_has_message_version,
    event_has_round_id,
    event_is_openark_kind_150,
    new_round_initiate_event_builder,
    new_round_join_event_builder,
    new_round_join_payload_json,
    event_round_id,
    parse_new_round_join_payload,
    parse_new_round_initiate_payload,
    validate_new_round_initiate_event,
    validate_new_round_join_event_reference,
};
pub use wallet::create_regtest_bdk_wallet;

pub const PROVIDER_MESSAGE: &str = "hello";
pub const USER_REPLY_MESSAGE: &str = "how are you today";
