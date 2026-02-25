use bdk_wallet::bitcoin::{
    absolute,
    bip32::{DerivationPath, Xpriv, Xpub},
    consensus::encode::serialize_hex,
    psbt::Psbt,
    secp256k1::Secp256k1,
    transaction,
    Address, Amount, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid,
    Witness,
};
use bdk_wallet::{KeychainKind, SignOptions, Wallet};
use openark_rs::print_tx_report;
use serde_json::{Value, json};
use std::str::FromStr;
use std::time::Duration;
use testcontainers::ImageExt;
use testcontainers::{
    ContainerAsync, GenericImage,
    core::IntoContainerPort,
    runners::AsyncRunner,
};

const RPC_USER: &str = "rpcuser";
const RPC_PASS: &str = "rpcpass";
const RPC_PORT: u16 = 18443;

const STEP2_SEND_TO_A_SATS: u64 = 100_000_000;
const STEP4_SEND_TO_AB_SATS: u64 = 99_980_000;
const STEP5_FEE_SATS: u64 = 20_000;

struct BitcoindHarness {
    _container: ContainerAsync<GenericImage>,
    rpc_url: String,
    client: reqwest::Client,
}

impl BitcoindHarness {
    async fn start() -> Self {
        let container = GenericImage::new("ruimarinho/bitcoin-core", "latest")
            .with_exposed_port(RPC_PORT.tcp())
            .with_cmd(vec![
                "-regtest=1".to_string(),
                "-server=1".to_string(),
                "-txindex=1".to_string(),
                "-printtoconsole".to_string(),
                "-fallbackfee=0.0002".to_string(),
                "-rpcbind=0.0.0.0".to_string(),
                "-rpcallowip=0.0.0.0/0".to_string(),
                format!("-rpcuser={RPC_USER}"),
                format!("-rpcpassword={RPC_PASS}"),
            ])
            .start()
            .await
            .expect("Failed to start bitcoind container");

        let host_port = container
            .get_host_port_ipv4(RPC_PORT)
            .await
            .expect("Failed to map bitcoind RPC port");

        let harness = Self {
            _container: container,
            rpc_url: format!("http://127.0.0.1:{host_port}"),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("Failed to build HTTP client"),
        };

        harness.wait_until_ready().await;
        harness
    }

    async fn wait_until_ready(&self) {
        for _ in 0..80 {
            if self.rpc_call("getblockchaininfo", json!([])).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        panic!("bitcoind RPC did not become ready in time");
    }

    async fn mine_blocks(&self, blocks: u64, address: &str) {
        self.rpc_call("generatetoaddress", json!([blocks, address]))
            .await
            .expect("generatetoaddress should succeed");
    }

    async fn broadcast_raw_tx(&self, hex: &str) -> String {
        self.rpc_call("sendrawtransaction", json!([hex]))
            .await
            .expect("sendrawtransaction should succeed")
            .as_str()
            .expect("txid should be string")
            .to_string()
    }

    async fn scan_address_utxo(&self, address: &str) -> (String, u64, u64) {
        let descriptor = format!("addr({address})");
        let result = self
            .rpc_call("scantxoutset", json!(["start", [descriptor]]))
            .await
            .expect("scantxoutset should succeed");

        let unspents = result["unspents"]
            .as_array()
            .expect("expected unspents array");
        let unspent = unspents
            .iter()
            .min_by_key(|u| u["height"].as_u64().unwrap_or(u64::MAX))
            .expect("expected at least one unspent for address");

        let txid = unspent["txid"]
            .as_str()
            .expect("txid should be string")
            .to_string();
        let vout = unspent["vout"].as_u64().expect("vout should be u64");
        let amount_btc = unspent["amount"].as_f64().expect("amount should be f64");
        let amount_sats = btc_to_sats(amount_btc);
        (txid, vout, amount_sats)
    }

    async fn scan_address_total_sats(&self, address: &str) -> u64 {
        let descriptor = format!("addr({address})");
        let result = self
            .rpc_call("scantxoutset", json!(["start", [descriptor]]))
            .await
            .expect("scantxoutset should succeed");
        let total_btc = result["total_amount"]
            .as_f64()
            .expect("total_amount should be f64");
        btc_to_sats(total_btc)
    }

    async fn get_raw_transaction_verbose(&self, txid: &str) -> Value {
        self.rpc_call("getrawtransaction", json!([txid, true]))
            .await
            .expect("getrawtransaction should succeed")
    }

    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({
            "jsonrpc": "1.0",
            "id": "test",
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.rpc_url)
            .basic_auth(RPC_USER, Some(RPC_PASS))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("RPC request failed for {method}: {e}"))?;

        let status = response.status();
        let json: Value = response
            .json()
            .await
            .map_err(|e| format!("RPC JSON decode failed for {method}: {e}"))?;

        if !status.is_success() {
            return Err(format!("RPC HTTP status error for {method}: {status}, body: {json}"));
        }
        if !json["error"].is_null() {
            return Err(format!("RPC returned error for {method}: {}", json["error"]));
        }
        Ok(json["result"].clone())
    }
}

fn btc_to_sats(btc: f64) -> u64 {
    (btc * 100_000_000.0).round() as u64
}

fn print_tx_outline(label: &str, tx: &Transaction) {
    print_tx_report(label, tx);
}

fn descriptor_key_exprs() -> (String, String, String, String, String, String, String) {
    let secp = Secp256k1::new();
    let master_xprv = Xpriv::from_str("tprv8ZgxMBicQKsPeZRHk4rTG6orPS2CRNFX3njhUXx5vj9qGog5ZMH4uGReDWN5kCkY3jmWEtWause41CDvBRXD1shKknAMKxT99o9qUTRVC6m")
        .expect("hardcoded test xprv should parse");

    let a_branch = master_xprv
        .derive_priv(&secp, &DerivationPath::from_str("m/0h").expect("valid path"))
        .expect("A branch derivation should succeed");
    let b_branch = master_xprv
        .derive_priv(&secp, &DerivationPath::from_str("m/1h").expect("valid path"))
        .expect("B branch derivation should succeed");
    let internal_branch = master_xprv
        .derive_priv(&secp, &DerivationPath::from_str("m/2h").expect("valid path"))
        .expect("internal branch derivation should succeed");

    let a_xpub = Xpub::from_priv(&secp, &a_branch);
    let b_xpub = Xpub::from_priv(&secp, &b_branch);
    let internal_xpub = Xpub::from_priv(&secp, &internal_branch);

    let a_priv = format!("{a_branch}/0/0");
    let b_priv = format!("{b_branch}/0/0");
    let a_pub = format!("{a_xpub}/0/0");
    let b_pub = format!("{b_xpub}/0/0");
    let internal_pub = format!("{internal_xpub}/0/0");
    let a_single = format!("tr({a_priv})");
    let b_single = format!("tr({b_priv})");
    (a_priv, b_priv, a_pub, b_pub, internal_pub, a_single, b_single)
}

fn build_single_input_two_output_psbt(
    prev_txid: &str,
    prev_vout: u64,
    prev_value_sats: u64,
    prev_spk: ScriptBuf,
    dest1_spk: ScriptBuf,
    dest1_sats: u64,
    dest2_spk: ScriptBuf,
    dest2_sats: u64,
) -> Psbt {
    let tx = Transaction {
        version: transaction::Version(2),
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::from_str(prev_txid).expect("valid txid"),
                vout: prev_vout as u32,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::new(),
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(dest1_sats),
                script_pubkey: dest1_spk,
            },
            TxOut {
                value: Amount::from_sat(dest2_sats),
                script_pubkey: dest2_spk,
            },
        ],
    };

    let mut psbt = Psbt::from_unsigned_tx(tx).expect("unsigned tx should create PSBT");
    psbt.inputs[0].witness_utxo = Some(TxOut {
        value: Amount::from_sat(prev_value_sats),
        script_pubkey: prev_spk,
    });
    psbt
}

fn build_single_input_one_output_psbt(
    prev_txid: &str,
    prev_vout: u64,
    prev_value_sats: u64,
    prev_spk: ScriptBuf,
    dest_spk: ScriptBuf,
    dest_sats: u64,
) -> Psbt {
    let tx = Transaction {
        version: transaction::Version(2),
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::from_str(prev_txid).expect("valid txid"),
                vout: prev_vout as u32,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(dest_sats),
            script_pubkey: dest_spk,
        }],
    };

    let mut psbt = Psbt::from_unsigned_tx(tx).expect("unsigned tx should create PSBT");
    psbt.inputs[0].witness_utxo = Some(TxOut {
        value: Amount::from_sat(prev_value_sats),
        script_pubkey: prev_spk,
    });
    psbt
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn regtest_rust_does_all_building_and_signing_bitcoind_broadcasts_only() {
    let bitcoind = BitcoindHarness::start().await;

    let (a_priv_keyexpr, b_priv_keyexpr, a_pub_keyexpr, b_pub_keyexpr, internal_pub_keyexpr, a_single_desc, b_single_desc) =
        descriptor_key_exprs();
    let miner_desc = {
        let secp = Secp256k1::new();
        let master_xprv = Xpriv::from_str("tprv8ZgxMBicQKsPeZRHk4rTG6orPS2CRNFX3njhUXx5vj9qGog5ZMH4uGReDWN5kCkY3jmWEtWause41CDvBRXD1shKknAMKxT99o9qUTRVC6m")
            .expect("hardcoded test xprv should parse");
        let miner_branch = master_xprv
            .derive_priv(&secp, &DerivationPath::from_str("m/3h").expect("valid path"))
            .expect("miner branch derivation should succeed");
        format!("tr({miner_branch}/0/0)")
    };

    let mut miner_wallet = Wallet::create_single(miner_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("miner wallet descriptor should be valid");
    let mut wallet_a_single = Wallet::create_single(a_single_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("A single-sig wallet should be valid");
    let mut wallet_b_single = Wallet::create_single(b_single_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("B single-sig wallet should be valid");

    let ab_watch_desc = format!(
        "tr({internal_pub_keyexpr},multi_a(2,{a_pub_keyexpr},{b_pub_keyexpr}))"
    );
    let ab_sign_desc_a = format!(
        "tr({internal_pub_keyexpr},multi_a(2,{a_priv_keyexpr},{b_pub_keyexpr}))"
    );
    let ab_sign_desc_b = format!(
        "tr({internal_pub_keyexpr},multi_a(2,{a_pub_keyexpr},{b_priv_keyexpr}))"
    );

    let mut wallet_ab_watch = Wallet::create_single(ab_watch_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("A+B watch wallet descriptor should be valid");
    let wallet_ab_sign_a = Wallet::create_single(ab_sign_desc_a)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("A signer A+B descriptor should be valid");
    let wallet_ab_sign_b = Wallet::create_single(ab_sign_desc_b)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("B signer A+B descriptor should be valid");

    let miner_address = miner_wallet
        .reveal_next_address(KeychainKind::External)
        .address
        .to_string();
    let a_address = wallet_a_single
        .reveal_next_address(KeychainKind::External)
        .address
        .to_string();
    let b_address = wallet_b_single
        .reveal_next_address(KeychainKind::External)
        .address
        .to_string();
    let ab_address = wallet_ab_watch
        .reveal_next_address(KeychainKind::External)
        .address
        .to_string();

    println!("[step1] mine blocks to rust-generated miner address");
    bitcoind.mine_blocks(120, &miner_address).await;

    println!("[step2] send 1 BTC to A with rust-built PSBT");
    let (miner_utxo_txid, miner_utxo_vout, miner_utxo_sats) = bitcoind.scan_address_utxo(&miner_address).await;
    let a_spk = Address::from_str(&a_address)
        .expect("valid A address")
        .require_network(Network::Regtest)
        .expect("A address should be regtest")
        .script_pubkey();
    let miner_change_spk = Address::from_str(&miner_address)
        .expect("valid miner address")
        .require_network(Network::Regtest)
        .expect("miner address should be regtest")
        .script_pubkey();
    let miner_input_spk = miner_change_spk.clone();
    let step2_fee = 20_000;
    let step2_change = miner_utxo_sats
        .checked_sub(STEP2_SEND_TO_A_SATS + step2_fee)
        .expect("miner utxo should cover send + fee");

    let mut step2_psbt = build_single_input_two_output_psbt(
        &miner_utxo_txid,
        miner_utxo_vout,
        miner_utxo_sats,
        miner_input_spk,
        a_spk,
        STEP2_SEND_TO_A_SATS,
        miner_change_spk,
        step2_change,
    );
    print_tx_outline("step2/unsigned", &step2_psbt.unsigned_tx);
    let step2_finalized = miner_wallet
        .sign(
            &mut step2_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: true,
                ..Default::default()
            },
        )
        .expect("miner signing should succeed");
    assert!(step2_finalized, "miner tx to A should finalize");
    let step2_tx = step2_psbt.extract_tx().expect("step2 extract tx");
    print_tx_outline("step2/final", &step2_tx);
    let step2_hex = serialize_hex(&step2_tx);
    println!("[tx/step2/final] txid={} hex={}", step2_tx.compute_txid(), step2_hex);
    let _step2_txid = bitcoind.broadcast_raw_tx(&step2_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    let (a_fund_txid, a_fund_vout, a_fund_sats) = bitcoind.scan_address_utxo(&a_address).await;
    assert_eq!(a_fund_sats, STEP2_SEND_TO_A_SATS, "A should receive 1 BTC in step2");

    println!("[step3] A+B taproot address created in rust: {ab_address}");

    println!("[step4] A funds A+B taproot output");
    let a_input_spk = Address::from_str(&a_address)
        .expect("valid A address")
        .require_network(Network::Regtest)
        .expect("A address should be regtest")
        .script_pubkey();
    let ab_spk = Address::from_str(&ab_address)
        .expect("valid A+B address")
        .require_network(Network::Regtest)
        .expect("A+B address should be regtest")
        .script_pubkey();

    let mut step4_psbt = build_single_input_one_output_psbt(
        &a_fund_txid,
        a_fund_vout,
        a_fund_sats,
        a_input_spk,
        ab_spk.clone(),
        STEP4_SEND_TO_AB_SATS,
    );
    print_tx_outline("step4/unsigned", &step4_psbt.unsigned_tx);
    let step4_finalized = wallet_a_single
        .sign(
            &mut step4_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: true,
                ..Default::default()
            },
        )
        .expect("A funding tx signing should succeed");
    assert!(step4_finalized, "A funding tx should finalize");
    let step4_tx = step4_psbt.extract_tx().expect("step4 extract tx");
    print_tx_outline("step4/final", &step4_tx);
    let step4_hex = serialize_hex(&step4_tx);
    println!("[tx/step4/final] txid={} hex={}", step4_tx.compute_txid(), step4_hex);
    let _step4_txid = bitcoind.broadcast_raw_tx(&step4_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    let (ab_fund_txid, ab_fund_vout, ab_fund_sats) = bitcoind.scan_address_utxo(&ab_address).await;
    assert_eq!(ab_fund_sats, STEP4_SEND_TO_AB_SATS, "A+B output should be funded");

    println!("[step5] A+B sign PSBT to spend A+B output to B");
    let b_spk = Address::from_str(&b_address)
        .expect("valid B address")
        .require_network(Network::Regtest)
        .expect("B address should be regtest")
        .script_pubkey();
    let step5_send_to_b = ab_fund_sats
        .checked_sub(STEP5_FEE_SATS)
        .expect("A+B input should cover fee");

    let mut step5_psbt = build_single_input_one_output_psbt(
        &ab_fund_txid,
        ab_fund_vout,
        ab_fund_sats,
        ab_spk,
        b_spk,
        step5_send_to_b,
    );
    print_tx_outline("step5/unsigned", &step5_psbt.unsigned_tx);

    let a_only_finalized = wallet_ab_sign_a
        .sign(
            &mut step5_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: false,
                ..Default::default()
            },
        )
        .expect("A partial signing should succeed");
    assert!(!a_only_finalized, "A-only should not finalize 2-of-2");

    let ab_finalized = wallet_ab_sign_b
        .sign(
            &mut step5_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: true,
                ..Default::default()
            },
        )
        .expect("B signing should complete PSBT");
    assert!(ab_finalized, "A+B should finalize 2-of-2 spend");

    let step5_tx = step5_psbt.extract_tx().expect("step5 extract tx");
    print_tx_outline("step5/final", &step5_tx);
    let step5_hex = serialize_hex(&step5_tx);
    println!("[tx/step5/final] txid={} hex={}", step5_tx.compute_txid(), step5_hex);
    let spend_to_b_txid = bitcoind.broadcast_raw_tx(&step5_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    println!("[step6] verify B got money");
    let b_total_sats = bitcoind.scan_address_total_sats(&b_address).await;
    assert_eq!(b_total_sats, step5_send_to_b, "B should receive full step5 output amount");

    let spend_verbose = bitcoind.get_raw_transaction_verbose(&spend_to_b_txid).await;
    let confirmations = spend_verbose["confirmations"].as_u64().unwrap_or(0);
    assert!(confirmations >= 1, "final spend tx must be confirmed");

    println!(
        "[done] rust built addresses/psbts/signatures; bitcoind handled chain state and broadcast only"
    );
}
