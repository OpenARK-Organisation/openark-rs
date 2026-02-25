#[allow(dead_code)]
mod support;

use support::{AspFundingPool, PreparedAspFundingUtxo};

#[test]
#[should_panic(expected = "insufficient available pool balance")]
fn reserve_fails_when_pool_is_underfunded() {
    let mut pool = AspFundingPool::new_with_min_confirmations(
        vec![PreparedAspFundingUtxo {
            outpoint: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:0"
                .to_string(),
            value_sats: 1_000,
            confirmations: 6,
        }],
        1,
    );

    let _ = pool.reserve_for_amount(2_000);
}

#[test]
#[should_panic(expected = "must be reserved to consume")]
fn consume_fails_on_double_use() {
    let mut pool = AspFundingPool::new_with_min_confirmations(
        vec![PreparedAspFundingUtxo {
            outpoint: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb:0"
                .to_string(),
            value_sats: 5_000,
            confirmations: 6,
        }],
        1,
    );

    let reserved = pool.reserve_for_amount(1_000);
    pool.consume(&reserved);
    pool.consume(&reserved);
}

#[test]
#[should_panic(expected = "insufficient available pool balance")]
fn reserve_rejects_unconfirmed_pool_utxo() {
    let mut pool = AspFundingPool::new_with_min_confirmations(
        vec![PreparedAspFundingUtxo {
            outpoint: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc:0"
                .to_string(),
            value_sats: 10_000,
            confirmations: 0,
        }],
        1,
    );

    let _ = pool.reserve_for_amount(1_000);
}
