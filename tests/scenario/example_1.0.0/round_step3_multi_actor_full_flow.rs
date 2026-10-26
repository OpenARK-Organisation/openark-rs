use bdk_wallet::bitcoin::{
    absolute,
    bip32::{DerivationPath, Xpriv, Xpub},
    consensus::encode::serialize_hex,
    psbt::Psbt,
    secp256k1::Secp256k1,
    transaction, Amount, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid,
    Witness, XOnlyPublicKey,
};
use bdk_wallet::{KeychainKind, SignOptions, Wallet};
use nostr_sdk::prelude::Event;
use openark_rs::{
    NEW_ROUND_INITIATE, NEW_ROUND_JOIN, NEW_ROUND_PREPARE_START, NEW_ROUND_START,
    NEW_ROUND_START_PREPARED, NEW_ROUND_VTXO_TREE_ACCEPT, NEW_ROUND_VTXO_TREE_PROPOSAL,
    NewRoundStartPayload, OpenArkServiceProvider, OpenArkUser, OutputScriptFactory,
    ROUND_MESSAGE_VERSION, derive_bip340_xonly_pubkey_from_seed_phrase,
    derive_master_xprv_base58_from_seed_phrase, event_event_reference, event_has_event_reference,
    event_has_message_type, event_has_message_version, event_is_openark_kind_150,
    parse_new_round_initiate_payload, parse_new_round_join_payload,
    parse_new_round_start_payload, parse_new_round_start_prepared_payload,
    parse_new_round_vtxo_tree_accept_payload, parse_new_round_vtxo_tree_proposal_payload,
    print_tx_report, validate_new_round_prepare_start_event, validate_new_round_start_event,
    validate_new_round_start_prepared_event, validate_new_round_vtxo_tree_accept_event,
    validate_new_round_vtxo_tree_proposal_event,
};
use std::str::FromStr;
use std::time::Duration;
mod support;
use support::{
    RegtestFundingHarness, TraceRecorder, canonical_join_payload,
    canonical_start_prepared_payload, canonical_vtxo_tree_accept_payload,
    canonical_vtxo_tree_proposal_payload, estimate_tree_proposal_fee_budget_sats,
    fund_wallets_two_btc, initialize_default_asp_funding_pool, prepare_onboard_utxos_one_btc,
    start_relay,
};

const ROUND_ID: &str = "r-2026-02-24-003";
const USERS: [&str; 5] = ["Alice", "Bob", "Carol", "Dave", "Eve"];

struct NamedUser {
    name: &'static str,
    user: OpenArkUser,
}

fn assert_round_event(
    event: &Event,
    message_type: &str,
    round_ref: Option<&nostr_sdk::prelude::EventId>,
) {
    assert!(event_is_openark_kind_150(event));
    assert!(event_has_message_type(event, message_type));
    if message_type == NEW_ROUND_INITIATE {
        assert!(event_has_message_version(event, ROUND_MESSAGE_VERSION));
        let payload = parse_new_round_initiate_payload(&event.content)
            .expect("initiate payload should parse");
        assert_eq!(payload.closing_block, 910000);
        assert_eq!(payload.recycled_block, 910144);
        assert_eq!(payload.leaf_timeout, 10);
    } else if message_type == NEW_ROUND_JOIN {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else if message_type == NEW_ROUND_VTXO_TREE_PROPOSAL {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(validate_new_round_vtxo_tree_proposal_event(event, round_ref).is_ok());
    } else if message_type == NEW_ROUND_PREPARE_START || message_type == NEW_ROUND_START_PREPARED {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
    } else if message_type == NEW_ROUND_START {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(validate_new_round_start_event(event, round_ref).is_ok());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn step3_multi_actor_kind150_full_phase_flow() {
    let mut trace = TraceRecorder::new("round_step3_multi_actor_full_flow", Some(ROUND_ID));
    println!("[test] step3_multi_actor_kind150_full_phase_flow started");
    let (_relay_container, relay_url) = start_relay().await;
    println!("[test] using relay {relay_url}");

    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    println!("[Steve] connected with pubkey={}", provider.pubkey);

    let mut users: Vec<NamedUser> = Vec::new();
    for name in USERS {
        let user = OpenArkUser::connect(&relay_url)
            .await
            .expect("user should connect");
        println!("[{name}] connected with pubkey={}", user.pubkey);
        users.push(NamedUser { name, user });
    }
    let mut wallets = vec![(
        "Steve",
        provider.wallet_receive_address(),
        provider.wallet_seed_phrase().to_string(),
    )];
    for named in &users {
        wallets.push((
            named.name,
            named.user.wallet_receive_address(),
            named.user.wallet_seed_phrase().to_string(),
        ));
    }
    fund_wallets_two_btc(&wallets).await;
    let mut asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let asp_pool_initial_available_count = asp_pool.available_count();
    let asp_pool_initial_available_value_sats = asp_pool.available_value_sats();
    let onboard_wallets: Vec<(&str, String, String)> = users
        .iter()
        .map(|named| {
            (
                named.name,
                named.user.wallet_receive_address(),
                named.user.wallet_seed_phrase().to_string(),
            )
        })
        .collect();
    let prepared_onboard = prepare_onboard_utxos_one_btc(&mut trace, &onboard_wallets).await;
    for named in &users {
        let prepared = prepared_onboard
            .get(named.name)
            .expect("prepared onboard utxo should exist for each user");
        assert_eq!(
            prepared.amount_sats, 100_000_000,
            "prepared onboard amount should be exactly 1 BTC"
        );
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("[test] clients settled, executing multi-actor phase sequence");

    let mut announcement_joins = 0usize;
    let mut tree_accepts = 0usize;
    let mut prepared_signals = 0usize;
    let mut activated_receipts = 0usize;

    println!("[phase] Round Announcement");
    trace.note("round_announcement", "provider publishing initiate");
    provider
        .send_new_round_initiate(910000, 910144, 10)
        .await
        .expect("provider should send new_round_initiate");
    println!("[Steve] published {NEW_ROUND_INITIATE}");
    let mut round_ref: Option<nostr_sdk::prelude::EventId> = None;

    for named in &users {
        let initiate = named
            .user
            .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
            .await
            .expect("user should wait for initiate")
            .expect("user should receive initiate");
        trace.event(named.name, "received initiate", &initiate);
        assert_round_event(&initiate, NEW_ROUND_INITIATE, None);
        if let Some(existing) = round_ref.as_ref() {
            assert_eq!(existing, &initiate.id);
        } else {
            round_ref = Some(initiate.id);
            println!("[round] reference={}", initiate.id);
        }
        let round_ref = round_ref.as_ref().expect("round ref should be present");

        let prepared = prepared_onboard
            .get(named.name)
            .expect("prepared onboard utxo should exist for join sender");
        let join_payload = canonical_join_payload(
            named.name,
            named.user.round_user_pubkey_hex(),
            prepared.outpoint.clone(),
        );
        named
            .user
            .send_new_round_join(round_ref, &join_payload)
            .await
            .expect("user should send join");
        println!("[{}] published {NEW_ROUND_JOIN}", named.name);
        let join = provider
            .wait_for_new_round_join_event(named.user.pubkey, round_ref, Duration::from_secs(10))
            .await
            .expect("provider should wait for join")
            .expect("provider should receive join");
        trace.event(
            "Steve",
            &format!("received join from {}", named.name),
            &join,
        );
        assert_round_event(&join, NEW_ROUND_JOIN, Some(round_ref));
        let join_payload =
            parse_new_round_join_payload(&join.content).expect("join payload should parse");
        assert_eq!(join_payload.onboard.len(), 1);
        assert_eq!(join_payload.onboard[0].outpoint, prepared.outpoint);
        assert_eq!(join_payload.offboard.len(), 0);
        assert_eq!(
            join_payload.keys.user_pubkey,
            named.user.round_user_pubkey_hex()
        );
        assert_eq!(
            join_payload.keys.agent_pubkey,
            format!("{}-agent-pubkey", named.name)
        );
        println!(
            "[round] join_event_reference={} actor={}",
            event_event_reference(&join).expect("join should have e reference"),
            named.name
        );
        announcement_joins += 1;
    }
    assert_eq!(announcement_joins, users.len());
    println!("[phase] Round Announcement complete: joins={announcement_joins}");

    println!("[phase] Tree Proposal");
    trace.note("tree_proposal", "provider publishing proposal");
    let proposal_fee_budget_sats = estimate_tree_proposal_fee_budget_sats(users.len());
    let reserved_pool_outpoints = asp_pool.reserve_for_amount(proposal_fee_budget_sats);
    trace.note(
        "asp_pool_reserve",
        &format!(
            "phase=tree_proposal fee_budget_sats={} reserved_outpoints={:?}",
            proposal_fee_budget_sats, reserved_pool_outpoints
        ),
    );
    println!(
        "[asp-pool] reserved_for_tree_proposal fee_budget_sats={} reserved_count={} reserved_value_sats={}",
        proposal_fee_budget_sats,
        reserved_pool_outpoints.len(),
        asp_pool.reserved_value_sats()
    );
    let proposal_payload = canonical_vtxo_tree_proposal_payload();
    let round_ref_for_proposal = round_ref.as_ref().expect("round ref should be present");
    provider
        .send_new_round_vtxo_tree_proposal(round_ref_for_proposal, &proposal_payload)
        .await
        .expect("provider should send proposal");
    asp_pool.consume(&reserved_pool_outpoints);
    trace.note(
        "asp_pool_consume",
        &format!(
            "phase=tree_proposal consumed_outpoints={:?} spent_value_sats={}",
            reserved_pool_outpoints,
            asp_pool.spent_value_sats()
        ),
    );
    println!(
        "[asp-pool] consumed_for_tree_proposal spent_count={} spent_value_sats={}",
        asp_pool.spent_count(),
        asp_pool.spent_value_sats()
    );
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    let mut proposal_ref: Option<nostr_sdk::prelude::EventId> = None;
    for named in &users {
        let proposal = named
            .user
            .wait_for_new_round_vtxo_tree_proposal_event(
                provider.pubkey,
                round_ref_for_proposal,
                Duration::from_secs(10),
            )
            .await
            .expect("user should wait for proposal")
            .expect("user should receive proposal");
        trace.event(named.name, "received proposal", &proposal);
        assert_round_event(
            &proposal,
            NEW_ROUND_VTXO_TREE_PROPOSAL,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        let parsed_proposal = parse_new_round_vtxo_tree_proposal_payload(&proposal.content)
            .expect("proposal should parse");
        assert_eq!(parsed_proposal.root_psbt, proposal_payload.root_psbt);
        if let Some(existing) = proposal_ref.as_ref() {
            assert_eq!(existing, &proposal.id);
        } else {
            proposal_ref = Some(proposal.id);
        }

        let accept_payload = canonical_vtxo_tree_accept_payload(named.name);
        named
            .user
            .send_new_round_vtxo_tree_accept(
                round_ref_for_proposal,
                proposal_ref
                    .as_ref()
                    .expect("proposal ref should be present"),
                &accept_payload,
            )
            .await
            .expect("user should send accept");
        println!("[{}] published {NEW_ROUND_VTXO_TREE_ACCEPT}", named.name);
        let accept = provider
            .wait_for_new_round_vtxo_tree_accept_event(
                named.user.pubkey,
                round_ref_for_proposal,
                proposal_ref
                    .as_ref()
                    .expect("proposal ref should be present"),
                Duration::from_secs(10),
            )
            .await
            .expect("provider should wait for accept")
            .expect("provider should receive accept");
        trace.event(
            "Steve",
            &format!("received accept from {}", named.name),
            &accept,
        );
        assert!(
            validate_new_round_vtxo_tree_accept_event(
                &accept,
                round_ref_for_proposal,
                proposal_ref
                    .as_ref()
                    .expect("proposal ref should be present"),
            )
            .is_ok()
        );
        let parsed_accept = parse_new_round_vtxo_tree_accept_payload(&accept.content)
            .expect("accept payload should parse");
        assert_eq!(parsed_accept.vtxo_tree_signatures.len(), 2);
        tree_accepts += 1;
    }
    assert_eq!(tree_accepts, users.len());
    println!("[phase] Tree Proposal complete: accepts={tree_accepts}");

    println!("[phase] Start Preparation");
    trace.note("start_preparation", "provider publishing prepare_start");
    let round_ref_for_prepare = round_ref.as_ref().expect("round ref should be present");
    provider
        .send_new_round_prepare_start(
            round_ref_for_prepare,
            proposal_ref
                .as_ref()
                .expect("proposal ref should be present"),
        )
        .await
        .expect("provider should send prepare_start");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    let mut prepare_start_ref: Option<nostr_sdk::prelude::EventId> = None;
    for named in &users {
        let prepare_start = named
            .user
            .wait_for_new_round_prepare_start_event(
                provider.pubkey,
                round_ref_for_prepare,
                proposal_ref
                    .as_ref()
                    .expect("proposal ref should be present"),
                Duration::from_secs(10),
            )
            .await
            .expect("user should wait for prepare_start")
            .expect("user should receive prepare_start");
        trace.event(named.name, "received prepare_start", &prepare_start);
        assert_round_event(
            &prepare_start,
            NEW_ROUND_PREPARE_START,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        assert!(
            validate_new_round_prepare_start_event(
                &prepare_start,
                round_ref_for_prepare,
                proposal_ref
                    .as_ref()
                    .expect("proposal ref should be present"),
            )
            .is_ok()
        );
        assert!(prepare_start.content.is_empty());
        if let Some(existing) = prepare_start_ref.as_ref() {
            assert_eq!(existing, &prepare_start.id);
        } else {
            prepare_start_ref = Some(prepare_start.id);
        }

        let prepared_payload = canonical_start_prepared_payload(named.name);
        named
            .user
            .send_new_round_start_prepared(
                round_ref_for_prepare,
                prepare_start_ref
                    .as_ref()
                    .expect("prepare_start ref should be present"),
                &prepared_payload,
            )
            .await
            .expect("user should send start_prepared");
        println!("[{}] published {NEW_ROUND_START_PREPARED}", named.name);
        let prepared = provider
            .wait_for_new_round_start_prepared_event(
                named.user.pubkey,
                round_ref_for_prepare,
                prepare_start_ref
                    .as_ref()
                    .expect("prepare_start ref should be present"),
                Duration::from_secs(10),
            )
            .await
            .expect("provider should wait for start_prepared")
            .expect("provider should receive start_prepared");
        trace.event(
            "Steve",
            &format!("received start_prepared from {}", named.name),
            &prepared,
        );
        assert_round_event(
            &prepared,
            NEW_ROUND_START_PREPARED,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        assert!(
            validate_new_round_start_prepared_event(
                &prepared,
                round_ref_for_prepare,
                prepare_start_ref
                    .as_ref()
                    .expect("prepare_start ref should be present"),
            )
            .is_ok()
        );
        assert!(parse_new_round_start_prepared_payload(&prepared.content).is_some());
        prepared_signals += 1;
    }
    assert_eq!(prepared_signals, users.len());
    println!("[phase] Start Preparation complete: prepared_signals={prepared_signals}");

    println!("[phase] Round Activation");
    trace.note("round_activation", "provider publishing round_start");
    provider
        .send_new_round_start(
            round_ref.as_ref().expect("round ref should be present"),
            &NewRoundStartPayload {
                round_tx: "BASE64_SIGNED_ROOT_TX_003".to_string(),
            },
        )
        .await
        .expect("provider should send round_start");
    println!("[Steve] published {NEW_ROUND_START}");

    for named in &users {
        let start = named
            .user
            .wait_for_new_round_start_event(
                provider.pubkey,
                round_ref.as_ref().expect("round ref should be present"),
                Duration::from_secs(10),
            )
            .await
            .expect("user should wait for round_start")
            .expect("user should receive round_start");
        trace.event(named.name, "received round_start", &start);
        assert_round_event(
            &start,
            NEW_ROUND_START,
            Some(round_ref.as_ref().expect("round ref should be present")),
        );
        let parsed_start =
            parse_new_round_start_payload(&start.content).expect("start payload should parse");
        assert_eq!(parsed_start.round_tx, "BASE64_SIGNED_ROOT_TX_003");
        activated_receipts += 1;
    }
    assert_eq!(activated_receipts, users.len());
    println!("[phase] Round Activation complete: receipts={activated_receipts}");
    trace.note(
        "asp_pool_summary",
        &format!(
            "initial_available_count={} initial_available_value_sats={} consumed_count={} consumed_value_sats={} remaining_count={} remaining_value_sats={}",
            asp_pool_initial_available_count,
            asp_pool_initial_available_value_sats,
            asp_pool.spent_count(),
            asp_pool.spent_value_sats(),
            asp_pool.available_count(),
            asp_pool.available_value_sats()
        ),
    );
    println!(
        "[asp-pool] summary initial_available_count={} initial_available_value_sats={} consumed_count={} consumed_value_sats={} remaining_count={} remaining_value_sats={}",
        asp_pool_initial_available_count,
        asp_pool_initial_available_value_sats,
        asp_pool.spent_count(),
        asp_pool.spent_value_sats(),
        asp_pool.available_count(),
        asp_pool.available_value_sats()
    );
    println!(
        "[asp-pool] final available={} reserved={} spent={} available_value_sats={} spent_value_sats={}",
        asp_pool.available_count(),
        asp_pool.reserved_count(),
        asp_pool.spent_count(),
        asp_pool.available_value_sats(),
        asp_pool.spent_value_sats()
    );

    println!(
        "[test] round protocol complete with {} users — beginning unilateral exit phase",
        users.len()
    );

    // ── Constants for exit ──────────────────────────────────────────────
    const LEAF_FUND_SATS: u64 = 50_000_000; // 0.5 BTC per user
    const EXIT_FEE_SATS: u64 = 20_000;
    const LEAF_TIMEOUT: u32 = 10;
    let exit_amount = LEAF_FUND_SATS
        .checked_sub(EXIT_FEE_SATS)
        .expect("leaf funds should cover exit fee");

    // ── Start persistent bitcoind harness ────────────────────────────────
    println!("[exit] starting regtest harness for unilateral exits");
    let harness = RegtestFundingHarness::start().await;
    let miner_address = harness.get_new_miner_address().await;
    harness.mine_blocks(101, &miner_address).await;

    // ── Derive Steve's (ASP) x-only pubkey ──────────────────────────────
    let steve_xonly_hex =
        derive_bip340_xonly_pubkey_from_seed_phrase(&provider.wallet_seed_phrase());
    let steve_xonly =
        XOnlyPublicKey::from_str(&steve_xonly_hex).expect("steve xonly should parse");
    println!("[exit] steve_xonly={steve_xonly_hex}");

    // ── Per-user: build leaf, fund, sign exit, collect hex ───────────────
    struct UserExitData {
        name: &'static str,
        leaf_address: String,
        receive_address: String,
        exit_hex: String,
    }

    let secp = Secp256k1::new();
    let factory = OutputScriptFactory::new(910144, LEAF_TIMEOUT);
    let mut exit_data: Vec<UserExitData> = Vec::new();

    for named in &users {
        let name = named.name;
        let user_xonly_hex = named.user.round_user_pubkey_hex();
        let user_xprv_base58 =
            derive_master_xprv_base58_from_seed_phrase(&named.user.wallet_seed_phrase());
        let user_xonly =
            XOnlyPublicKey::from_str(&user_xonly_hex).expect("user xonly should parse");
        println!("[exit:{name}] user_xonly={user_xonly_hex}");

        // Derive account-level xprv/xpub for BDK key expressions.
        let master = Xpriv::from_str(&user_xprv_base58)
            .expect("user master xprv should parse");
        let account_path =
            DerivationPath::from_str("m/86h/1h/0h").expect("valid derivation path");
        let account_xprv = master
            .derive_priv(&secp, &account_path)
            .expect("account-level derivation should succeed");
        let account_xpub = Xpub::from_priv(&secp, &account_xprv);
        let priv_keyexpr = format!("{account_xprv}/0/0");
        let pub_keyexpr = format!("{account_xpub}/0/0");

        // Build BDK descriptors.
        let leaf_sign_desc = format!(
            "tr({steve_xonly_hex},{{multi_a(2,{pub_keyexpr},{steve_xonly_hex}),and_v(v:older({LEAF_TIMEOUT}),pk({priv_keyexpr}))}})"
        );
        let leaf_watch_desc = format!(
            "tr({steve_xonly_hex},{{multi_a(2,{pub_keyexpr},{steve_xonly_hex}),and_v(v:older({LEAF_TIMEOUT}),pk({pub_keyexpr}))}})"
        );

        let mut wallet_leaf_sign = Wallet::create_single(leaf_sign_desc)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .expect("leaf signing wallet descriptor should be valid");
        let mut wallet_leaf_watch = Wallet::create_single(leaf_watch_desc)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .expect("leaf watch wallet descriptor should be valid");

        wallet_leaf_sign.reveal_next_address(KeychainKind::External);
        let leaf_address = wallet_leaf_watch
            .reveal_next_address(KeychainKind::External)
            .address;

        // Verify address matches OutputScriptFactory.
        let factory_spec = factory
            .create_leaf_output(user_xonly, steve_xonly)
            .expect("create_leaf_output should succeed");
        assert_eq!(
            factory_spec.script_pubkey,
            leaf_address.script_pubkey(),
            "OutputScriptFactory and BDK descriptor must match for {name}"
        );
        println!("[exit:{name}] leaf_address={leaf_address} (factory match confirmed)");

        // Fund the leaf output.
        let leaf_address_str = leaf_address.to_string();
        let leaf_fund_btc = LEAF_FUND_SATS as f64 / 100_000_000.0;
        harness
            .send_to_address(&leaf_address_str, leaf_fund_btc)
            .await;
        harness.mine_blocks(1, &miner_address).await;

        let (leaf_txid, leaf_vout, leaf_sats) =
            harness.scan_address_utxo(&leaf_address_str).await;
        assert_eq!(
            leaf_sats, LEAF_FUND_SATS,
            "{name} leaf output should be funded with 0.5 BTC"
        );
        println!("[exit:{name}] leaf funded: txid={leaf_txid} vout={leaf_vout} sats={leaf_sats}");

        // Build user's receive address.
        let mut user_wallet = openark_rs::create_regtest_bdk_wallet_from_seed_phrase(
            &named.user.wallet_seed_phrase(),
        );
        let receive_address = user_wallet
            .reveal_next_address(KeychainKind::External)
            .address;
        let receive_spk = receive_address.script_pubkey();

        // Build and sign exit PSBT.
        let exit_tx = Transaction {
            version: transaction::Version(2),
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_str(&leaf_txid).expect("valid leaf txid"),
                    vout: leaf_vout as u32,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence(LEAF_TIMEOUT),
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(exit_amount),
                script_pubkey: receive_spk,
            }],
        };

        let mut exit_psbt =
            Psbt::from_unsigned_tx(exit_tx).expect("unsigned exit tx should create PSBT");
        exit_psbt.inputs[0].witness_utxo = Some(TxOut {
            value: Amount::from_sat(LEAF_FUND_SATS),
            script_pubkey: leaf_address.script_pubkey(),
        });
        print_tx_report(&format!("exit:{name}/unsigned"), &exit_psbt.unsigned_tx);

        let exit_finalized = wallet_leaf_sign
            .sign(
                &mut exit_psbt,
                SignOptions {
                    trust_witness_utxo: true,
                    try_finalize: true,
                    ..Default::default()
                },
            )
            .expect("exit PSBT signing should succeed");
        assert!(
            exit_finalized,
            "{name} exit PSBT should finalize with timeout path key"
        );

        let exit_final_tx = exit_psbt.extract_tx().expect("exit extract tx");
        print_tx_report(&format!("exit:{name}/final"), &exit_final_tx);
        let exit_hex = serialize_hex(&exit_final_tx);

        // Pre-timeout: broadcast must fail.
        let pre_timeout = harness.try_broadcast_raw_tx(&exit_hex).await;
        assert!(
            pre_timeout.is_err(),
            "{name} pre-timeout broadcast must be rejected as non-final"
        );
        println!("[exit:{name}] pre-timeout correctly rejected");

        exit_data.push(UserExitData {
            name,
            leaf_address: leaf_address_str,
            receive_address: receive_address.to_string(),
            exit_hex,
        });
    }

    // ── Mine blocks to satisfy CSV for all users ────────────────────────
    println!("[exit] mining {LEAF_TIMEOUT} blocks to satisfy CSV for all users");
    harness
        .mine_blocks(LEAF_TIMEOUT as u64, &miner_address)
        .await;

    // ── Broadcast all exit transactions ─────────────────────────────────
    let mut exit_results: Vec<serde_json::Value> = Vec::new();
    for data in &exit_data {
        let exit_txid = harness.broadcast_raw_tx(&data.exit_hex).await;
        println!("[exit:{}] exit tx broadcast: txid={exit_txid}", data.name);
        exit_results.push(serde_json::json!({
            "name": data.name,
            "exit_txid": exit_txid,
            "leaf_address": data.leaf_address,
            "receive_address": data.receive_address,
        }));
    }

    // ── Confirm and verify all users received funds ─────────────────────
    harness.mine_blocks(1, &miner_address).await;
    for data in &exit_data {
        let received_sats = harness.scan_address_total_sats(&data.receive_address).await;
        assert_eq!(
            received_sats, exit_amount,
            "{} should receive LEAF_FUND_SATS - EXIT_FEE_SATS after unilateral exit",
            data.name
        );
        println!(
            "[exit:{}] received {received_sats} sats at {} (expected {exit_amount})",
            data.name, data.receive_address
        );
    }

    println!(
        "[test] all {} users verified unilateral exit via CSV timeout path",
        users.len()
    );
    println!("[test] step3_multi_actor_kind150_full_phase_flow passed");
    trace.finish(
        "passed",
        serde_json::json!({
            "announcement_joins": announcement_joins,
            "tree_accepts": tree_accepts,
            "prepared_signals": prepared_signals,
            "activated_receipts": activated_receipts,
            "exit_results": exit_results,
            "exit_amount_sats": exit_amount,
        }),
    );
}
