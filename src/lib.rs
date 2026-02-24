pub mod actors;
pub mod round;
pub mod wallet;

pub use actors::{OpenArkServiceProvider, OpenArkUser};
pub use round::{
    NEW_ROUND_INITIATE,
    NEW_ROUND_JOIN,
    NEW_ROUND_PREPARE_START,
    NEW_ROUND_START,
    NEW_ROUND_START_PREPARED,
    NEW_ROUND_VTXO_TREE_ACCEPT,
    NEW_ROUND_VTXO_TREE_PROPOSAL,
    OPENARK_KIND_150,
    event_has_message_type,
    event_has_round_id,
    event_is_openark_kind_150,
    event_round_id,
};
pub use wallet::create_regtest_bdk_wallet;

pub const PROVIDER_MESSAGE: &str = "hello";
pub const USER_REPLY_MESSAGE: &str = "how are you today";
