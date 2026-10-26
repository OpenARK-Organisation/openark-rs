use bdk_wallet::bitcoin::{
    absolute,
    consensus::encode::serialize_hex,
    psbt::Psbt,
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
    print_tx_report, validate_new_round_initiate_event, validate_new_round_join_event_reference,
    validate_new_round_prepare_start_event, validate_new_round_start_event,
    validate_new_round_start_prepared_event, validate_new_round_vtxo_tree_accept_event,
    validate_new_round_vtxo_tree_proposal_event,
};
use std::str::FromStr;
use std::time::Duration;
mod support;
use support::{
    PREPARE_FUNDING_OUTPOINT_0, PROPOSAL_VTXO_OUTPOINT_0, RegtestFundingHarness, TraceRecorder,
    canonical_join_payload, canonical_start_prepared_payload, canonical_vtxo_tree_accept_payload,
    canonical_vtxo_tree_proposal_payload, fund_wallets_two_btc,
    initialize_default_asp_funding_pool, prepare_onboard_utxos_one_btc, start_relay,
};

const ROUND_ID: &str = "r-2026-02-24-002";

fn assert_round_event(
    event: &Event,
    message_type: &str,
    round_ref: Option<&nostr_sdk::prelude::EventId>,
) {
    assert!(event_is_openark_kind_150(event));
    assert!(event_has_message_type(event, message_type));
    if message_type == NEW_ROUND_INITIATE {
        assert!(event_has_message_version(event, ROUND_MESSAGE_VERSION));
        assert!(validate_new_round_initiate_event(event).is_ok());
        let payload = parse_new_round_initiate_payload(&event.content)
            .expect("initiate payload should parse");
        assert_eq!(payload.closing_block, 910000);
        assert_eq!(payload.recycled_block, 910144);
        assert_eq!(payload.leaf_timeout, 10);
    } else if message_type == NEW_ROUND_JOIN {
        let round_ref = round_ref.expect("round ref should be set");
        assert!(event_has_event_reference(event, round_ref));
        assert!(validate_new_round_join_event_reference(event, round_ref).is_ok());
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn step2_two_actor_kind150_full_phase_flow() {
    let mut trace = TraceRecorder::new("round_step2_two_actor_full_flow", Some(ROUND_ID));
    println!("[test] step2_two_actor_kind150_full_phase_flow started");
    let (_relay_container, relay_url) = start_relay().await;
    println!("[test] using relay {relay_url}");

    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    println!("[Steve] connected with pubkey={}", provider.pubkey);

    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");
    println!("[Alice] connected with pubkey={}", user.pubkey);
    fund_wallets_two_btc(&[
        (
            "Steve",
            provider.wallet_receive_address(),
            provider.wallet_seed_phrase().to_string(),
        ),
        (
            "Alice",
            user.wallet_receive_address(),
            user.wallet_seed_phrase().to_string(),
        ),
    ])
    .await;
    let _asp_pool = initialize_default_asp_funding_pool(
        &mut trace,
        "Steve",
        &provider.wallet_receive_address(),
    )
    .await;
    let prepared_onboard = prepare_onboard_utxos_one_btc(
        &mut trace,
        &[(
            "Alice",
            user.wallet_receive_address(),
            user.wallet_seed_phrase().to_string(),
        )],
    )
    .await;
    let alice_onboard = prepared_onboard
        .get("Alice")
        .expect("Alice onboard utxo should be prepared");
    assert_eq!(
        alice_onboard.amount_sats, 100_000_000,
        "Alice prepared onboard utxo must be exactly 1 BTC"
    );

    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("[test] clients settled, executing phase sequence");

    let mut observed_order: Vec<&'static str> = Vec::new();

    println!("[phase] Round Announcement");
    trace.note("round_announcement", "provider publishing initiate");
    provider
        .send_new_round_initiate(910000, 910144, 10)
        .await
        .expect("provider should send new_round_initiate");
    println!("[Steve] published {NEW_ROUND_INITIATE}");

    let initiate = user
        .wait_for_new_round_initiate_event(provider.pubkey, Duration::from_secs(10))
        .await
        .expect("user should wait for initiate")
        .expect("user should receive initiate");
    trace.event("Alice", "received initiate", &initiate);
    assert_round_event(&initiate, NEW_ROUND_INITIATE, None);
    let round_ref = initiate.id;
    println!("[round] reference={round_ref}");
    observed_order.push(NEW_ROUND_INITIATE);

    let join_payload = canonical_join_payload(
        "Alice",
        user.round_user_pubkey_hex(),
        alice_onboard.outpoint.clone(),
    );
    user.send_new_round_join(&round_ref, &join_payload)
        .await
        .expect("user should send join");
    println!("[Alice] published {NEW_ROUND_JOIN}");

    let join = provider
        .wait_for_new_round_join_event(user.pubkey, &round_ref, Duration::from_secs(10))
        .await
        .expect("provider should wait for join")
        .expect("provider should receive join");
    trace.event("Steve", "received join", &join);
    assert_round_event(&join, NEW_ROUND_JOIN, Some(&round_ref));
    println!(
        "[round] join_event_reference={}",
        event_event_reference(&join).expect("join should have e reference")
    );
    let join_payload = parse_new_round_join_payload(&join.content)
        .expect("join content should parse as canonical JSON payload");
    assert_eq!(join_payload.onboard.len(), 1);
    assert_eq!(
        join_payload.onboard[0].outpoint, alice_onboard.outpoint,
        "join payload onboard outpoint must match prepared Alice onboard utxo"
    );
    assert_eq!(join_payload.offboard.len(), 0);
    assert_eq!(join_payload.keys.user_pubkey, user.round_user_pubkey_hex());
    assert_eq!(join_payload.keys.agent_pubkey, "Alice-agent-pubkey");
    observed_order.push(NEW_ROUND_JOIN);

    println!("[phase] Tree Proposal");
    trace.note("tree_proposal", "provider publishing proposal");
    let proposal_payload = canonical_vtxo_tree_proposal_payload();
    provider
        .send_new_round_vtxo_tree_proposal(&round_ref, &proposal_payload)
        .await
        .expect("provider should send proposal");
    println!("[Steve] published {NEW_ROUND_VTXO_TREE_PROPOSAL}");

    let proposal = user
        .wait_for_new_round_vtxo_tree_proposal_event(
            provider.pubkey,
            &round_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("user should wait for proposal")
        .expect("user should receive proposal");
    trace.event("Alice", "received proposal", &proposal);
    assert_round_event(&proposal, NEW_ROUND_VTXO_TREE_PROPOSAL, Some(&round_ref));
    let parsed_proposal = parse_new_round_vtxo_tree_proposal_payload(&proposal.content)
        .expect("proposal should parse");
    assert_eq!(parsed_proposal.root_psbt, proposal_payload.root_psbt);
    assert_eq!(
        parsed_proposal.vtxo_tree_psbts,
        proposal_payload.vtxo_tree_psbts
    );
    assert_eq!(
        parsed_proposal.forfeit_tree_psbts,
        proposal_payload.forfeit_tree_psbts
    );
    assert_eq!(parsed_proposal.recycle_psbt, proposal_payload.recycle_psbt);
    let proposal_ref = proposal.id;
    observed_order.push(NEW_ROUND_VTXO_TREE_PROPOSAL);

    let accept_payload = canonical_vtxo_tree_accept_payload("Alice");
    user.send_new_round_vtxo_tree_accept(&round_ref, &proposal_ref, &accept_payload)
        .await
        .expect("user should send accept");
    println!("[Alice] published {NEW_ROUND_VTXO_TREE_ACCEPT}");

    let accept = provider
        .wait_for_new_round_vtxo_tree_accept_event(
            user.pubkey,
            &round_ref,
            &proposal_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("provider should wait for accept")
        .expect("provider should receive accept");
    trace.event("Steve", "received accept", &accept);
    assert!(validate_new_round_vtxo_tree_accept_event(&accept, &round_ref, &proposal_ref).is_ok());
    let parsed_accept = parse_new_round_vtxo_tree_accept_payload(&accept.content)
        .expect("accept payload should parse");
    assert_eq!(
        parsed_accept
            .vtxo_tree_signatures
            .get(PROPOSAL_VTXO_OUTPOINT_0)
            .expect("signature for txid_0"),
        "BASE64_SIG_V0_Alice"
    );
    observed_order.push(NEW_ROUND_VTXO_TREE_ACCEPT);

    println!("[phase] Start Preparation");
    trace.note("start_preparation", "provider publishing prepare_start");
    provider
        .send_new_round_prepare_start(&round_ref, &proposal_ref)
        .await
        .expect("provider should send prepare_start");
    println!("[Steve] published {NEW_ROUND_PREPARE_START}");

    let prepare_start = user
        .wait_for_new_round_prepare_start_event(
            provider.pubkey,
            &round_ref,
            &proposal_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("user should wait for prepare_start")
        .expect("user should receive prepare_start");
    trace.event("Alice", "received prepare_start", &prepare_start);
    assert_round_event(&prepare_start, NEW_ROUND_PREPARE_START, Some(&round_ref));
    assert!(
        validate_new_round_prepare_start_event(&prepare_start, &round_ref, &proposal_ref).is_ok()
    );
    assert!(prepare_start.content.is_empty());
    let prepare_start_ref = prepare_start.id;
    observed_order.push(NEW_ROUND_PREPARE_START);

    let prepared_payload = canonical_start_prepared_payload("Alice");
    user.send_new_round_start_prepared(&round_ref, &prepare_start_ref, &prepared_payload)
        .await
        .expect("user should send start_prepared");
    println!("[Alice] published {NEW_ROUND_START_PREPARED}");

    let prepared = provider
        .wait_for_new_round_start_prepared_event(
            user.pubkey,
            &round_ref,
            &prepare_start_ref,
            Duration::from_secs(10),
        )
        .await
        .expect("provider should wait for start_prepared")
        .expect("provider should receive start_prepared");
    trace.event("Steve", "received start_prepared", &prepared);
    assert_round_event(&prepared, NEW_ROUND_START_PREPARED, Some(&round_ref));
    assert!(
        validate_new_round_start_prepared_event(&prepared, &round_ref, &prepare_start_ref).is_ok()
    );
    let parsed_prepared = parse_new_round_start_prepared_payload(&prepared.content)
        .expect("start_prepared payload should parse");
    assert_eq!(
        parsed_prepared
            .funding_signatures
            .get(PREPARE_FUNDING_OUTPOINT_0)
            .expect("funding signature for txid"),
        "BASE64_FUNDING_SIG_Alice"
    );
    observed_order.push(NEW_ROUND_START_PREPARED);

    println!("[phase] Round Activation");
    trace.note("round_activation", "provider publishing round_start");
    provider
        .send_new_round_start(
            &round_ref,
            &NewRoundStartPayload {
                round_tx: "BASE64_SIGNED_ROOT_TX_002".to_string(),
            },
        )
        .await
        .expect("provider should send round_start");
    println!("[Steve] published {NEW_ROUND_START}");

    let start = user
        .wait_for_new_round_start_event(provider.pubkey, &round_ref, Duration::from_secs(10))
        .await
        .expect("user should wait for round_start")
        .expect("user should receive round_start");
    trace.event("Alice", "received round_start", &start);
    assert_round_event(&start, NEW_ROUND_START, Some(&round_ref));
    let parsed_start =
        parse_new_round_start_payload(&start.content).expect("start payload should parse");
    assert_eq!(parsed_start.round_tx, "BASE64_SIGNED_ROOT_TX_002");
    observed_order.push(NEW_ROUND_START);

    let expected_order = vec![
        NEW_ROUND_INITIATE,
        NEW_ROUND_JOIN,
        NEW_ROUND_VTXO_TREE_PROPOSAL,
        NEW_ROUND_VTXO_TREE_ACCEPT,
        NEW_ROUND_PREPARE_START,
        NEW_ROUND_START_PREPARED,
        NEW_ROUND_START,
    ];
    assert_eq!(observed_order, expected_order);

    println!("[test] round protocol complete — beginning unilateral exit phase");

    // ── Constants for exit ──────────────────────────────────────────────
    const LEAF_FUND_SATS: u64 = 50_000_000; // 0.5 BTC
    const EXIT_FEE_SATS: u64 = 20_000;
    const LEAF_TIMEOUT: u32 = 10;

    // ── 4a. Start persistent bitcoind harness ───────────────────────────
    println!("[exit] starting regtest harness for unilateral exit");
    let harness = RegtestFundingHarness::start().await;
    let miner_address = harness.get_new_miner_address().await;
    harness.mine_blocks(101, &miner_address).await;

    // ── 4b. Derive keys for the leaf descriptor ─────────────────────────
    let alice_xonly_hex = user.round_user_pubkey_hex();
    let steve_xonly_hex =
        derive_bip340_xonly_pubkey_from_seed_phrase(&provider.wallet_seed_phrase());
    let alice_xprv_base58 =
        derive_master_xprv_base58_from_seed_phrase(&user.wallet_seed_phrase());
    println!("[exit] alice_xonly={alice_xonly_hex}");
    println!("[exit] steve_xonly={steve_xonly_hex}");
    println!("[exit] alice_xprv={}", &alice_xprv_base58[..20]);

    // ── 4c. Build BDK descriptor for the VTXO leaf ──────────────────────
    // Leaf structure: tr(asp_key, {multi_a(2, user_key, asp_key), csv_pk(leaf_timeout, user_key)})
    // BDK signing descriptor — Alice's xprv is only in the timeout path, so BDK selects that path.
    // Pre-derive Alice's account xprv/xpub (m/86h/1h/0h) so the descriptor uses only
    // non-hardened derivation steps. BDK's signer requires xpub-based key expressions
    // (not raw x-only hex) to match keys for tapscript signing.
    let (alice_priv_keyexpr, alice_pub_keyexpr) = {
        use bdk_wallet::bitcoin::bip32::{DerivationPath, Xpriv, Xpub};
        use bdk_wallet::bitcoin::secp256k1::Secp256k1;
        let secp = Secp256k1::new();
        let master = Xpriv::from_str(&alice_xprv_base58)
            .expect("alice master xprv should parse");
        let path = DerivationPath::from_str("m/86h/1h/0h").expect("valid derivation path");
        let account_xprv = master
            .derive_priv(&secp, &path)
            .expect("account-level derivation should succeed");
        let account_xpub = Xpub::from_priv(&secp, &account_xprv);
        (
            format!("{account_xprv}/0/0"),
            format!("{account_xpub}/0/0"),
        )
    };
    let leaf_sign_desc = format!(
        "tr({steve_xonly_hex},{{multi_a(2,{alice_pub_keyexpr},{steve_xonly_hex}),and_v(v:older({LEAF_TIMEOUT}),pk({alice_priv_keyexpr}))}})"
    );
    let leaf_watch_desc = format!(
        "tr({steve_xonly_hex},{{multi_a(2,{alice_pub_keyexpr},{steve_xonly_hex}),and_v(v:older({LEAF_TIMEOUT}),pk({alice_pub_keyexpr}))}})"
    );
    println!("[exit] leaf_sign_desc={leaf_sign_desc}");

    let mut wallet_leaf_sign = Wallet::create_single(leaf_sign_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("leaf signing wallet descriptor should be valid");
    let mut wallet_leaf_watch = Wallet::create_single(leaf_watch_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("leaf watch wallet descriptor should be valid");

    // Reveal addresses so BDK can match PSBT inputs against the descriptor.
    let leaf_address_from_sign = wallet_leaf_sign
        .reveal_next_address(KeychainKind::External)
        .address;
    let leaf_address = wallet_leaf_watch
        .reveal_next_address(KeychainKind::External)
        .address;
    assert_eq!(
        leaf_address_from_sign.script_pubkey(),
        leaf_address.script_pubkey(),
        "sign and watch wallets must produce the same leaf address"
    );
    println!("[exit] leaf_address={leaf_address}");

    // ── 4d. Verify address matches OutputScriptFactory ──────────────────
    let alice_xonly = XOnlyPublicKey::from_str(&alice_xonly_hex)
        .expect("alice xonly should parse");
    let steve_xonly = XOnlyPublicKey::from_str(&steve_xonly_hex)
        .expect("steve xonly should parse");
    let factory_spec = OutputScriptFactory::new(910144, LEAF_TIMEOUT)
        .create_leaf_output(alice_xonly, steve_xonly)
        .expect("create_leaf_output should succeed");
    let factory_spk = factory_spec.script_pubkey;
    let descriptor_spk = leaf_address.script_pubkey();
    assert_eq!(
        factory_spk, descriptor_spk,
        "OutputScriptFactory and BDK miniscript descriptor must produce identical Taproot script_pubkey"
    );
    println!("[exit] script_pubkey match confirmed between OutputScriptFactory and BDK descriptor");

    // ── 4e. Fund the leaf output on regtest ─────────────────────────────
    let leaf_address_str = leaf_address.to_string();
    let leaf_fund_btc = LEAF_FUND_SATS as f64 / 100_000_000.0;
    harness.send_to_address(&leaf_address_str, leaf_fund_btc).await;
    harness.mine_blocks(1, &miner_address).await;

    let (leaf_txid, leaf_vout, leaf_sats) = harness.scan_address_utxo(&leaf_address_str).await;
    assert_eq!(leaf_sats, LEAF_FUND_SATS, "leaf output should be funded with 0.5 BTC");
    println!("[exit] leaf funded: txid={leaf_txid} vout={leaf_vout} sats={leaf_sats}");

    // ── 4f. Build and sign the exit PSBT ────────────────────────────────
    // Alice's receive address is the exit destination.
    let mut alice_wallet = openark_rs::create_regtest_bdk_wallet_from_seed_phrase(
        &user.wallet_seed_phrase(),
    );
    let alice_receive_address = alice_wallet
        .reveal_next_address(KeychainKind::External)
        .address;
    let alice_receive_spk = alice_receive_address.script_pubkey();
    let exit_amount = LEAF_FUND_SATS.checked_sub(EXIT_FEE_SATS)
        .expect("leaf funds should cover exit fee");

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
            script_pubkey: alice_receive_spk.clone(),
        }],
    };

    let mut exit_psbt = Psbt::from_unsigned_tx(exit_tx).expect("unsigned exit tx should create PSBT");
    exit_psbt.inputs[0].witness_utxo = Some(TxOut {
        value: Amount::from_sat(LEAF_FUND_SATS),
        script_pubkey: descriptor_spk,
    });
    print_tx_report("exit/unsigned", &exit_psbt.unsigned_tx);

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
    assert!(exit_finalized, "exit PSBT should finalize with Alice's timeout path key");

    let exit_final_tx = exit_psbt.extract_tx().expect("exit extract tx");
    print_tx_report("exit/final", &exit_final_tx);
    let exit_hex = serialize_hex(&exit_final_tx);

    // ── 4g. Verify CSV enforcement ──────────────────────────────────────
    println!("[exit] pre-timeout broadcast must fail");
    let pre_timeout = harness.try_broadcast_raw_tx(&exit_hex).await;
    assert!(
        pre_timeout.is_err(),
        "pre-timeout broadcast must be rejected as non-final"
    );
    println!("[exit] pre-timeout correctly rejected");

    println!("[exit] mining {LEAF_TIMEOUT} blocks to satisfy CSV");
    harness.mine_blocks(LEAF_TIMEOUT as u64, &miner_address).await;

    println!("[exit] post-timeout broadcast must succeed");
    let exit_txid = harness.broadcast_raw_tx(&exit_hex).await;
    println!("[exit] exit tx broadcast: txid={exit_txid}");

    // ── 4h. Verify Alice received funds ─────────────────────────────────
    harness.mine_blocks(1, &miner_address).await;
    let alice_receive_str = alice_receive_address.to_string();
    let alice_exit_sats = harness.scan_address_total_sats(&alice_receive_str).await;
    assert_eq!(
        alice_exit_sats, exit_amount,
        "Alice should receive LEAF_FUND_SATS - EXIT_FEE_SATS after unilateral exit"
    );
    println!(
        "[exit] Alice received {alice_exit_sats} sats at {alice_receive_str} (expected {exit_amount})"
    );

    println!("[test] unilateral exit verified — Alice recovered funds via CSV timeout path");
    println!("[test] step2_two_actor_kind150_full_phase_flow passed");
    trace.finish(
        "passed",
        serde_json::json!({
            "observed_order": observed_order,
            "expected_order": expected_order,
            "exit_txid": exit_txid,
            "exit_amount_sats": exit_amount,
            "leaf_address": leaf_address_str,
            "alice_receive_address": alice_receive_str,
        }),
    );
}
