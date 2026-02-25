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

const FUND_TO_A_SATS: u64 = 200_000_000;
const FUND_SHARED_SATS: u64 = 99_980_000;
const FEE_SATS: u64 = 20_000;

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

    async fn get_block_count(&self) -> u64 {
        self.rpc_call("getblockcount", json!([]))
            .await
            .expect("getblockcount should succeed")
            .as_u64()
            .expect("getblockcount should return u64")
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

    async fn try_broadcast_raw_tx(&self, hex: &str) -> Result<String, String> {
        self.rpc_call("sendrawtransaction", json!([hex]))
            .await
            .map(|v| v.as_str().unwrap_or_default().to_string())
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
    sequence: Sequence,
    lock_time: absolute::LockTime,
) -> Psbt {
    let tx = Transaction {
        version: transaction::Version(2),
        lock_time,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::from_str(prev_txid).expect("valid txid"),
                vout: prev_vout as u32,
            },
            script_sig: ScriptBuf::new(),
            sequence,
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
    sequence: Sequence,
    lock_time: absolute::LockTime,
) -> Psbt {
    let tx = Transaction {
        version: transaction::Version(2),
        lock_time,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::from_str(prev_txid).expect("valid txid"),
                vout: prev_vout as u32,
            },
            script_sig: ScriptBuf::new(),
            sequence,
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
async fn regtest_rust_ab_or_b_after_t_paths() {
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
        .expect("A single wallet descriptor should be valid");
    let mut wallet_b_single = Wallet::create_single(b_single_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("B single wallet descriptor should be valid");

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

    println!("[step1] mine blocks to rust-generated miner address");
    bitcoind.mine_blocks(120, &miner_address).await;

    let current_height = bitcoind.get_block_count().await as u32;
    let lock_height = current_height + 10;
    println!("[setup] absolute timeout t={} (current={})", lock_height, current_height);

    let shared_watch_desc = format!(
        "tr({internal_pub_keyexpr},{{multi_a(2,{a_pub_keyexpr},{b_pub_keyexpr}),and_v(v:after({lock_height}),pk({b_pub_keyexpr}))}})"
    );
    let shared_sign_a_desc = format!(
        "tr({internal_pub_keyexpr},{{multi_a(2,{a_priv_keyexpr},{b_pub_keyexpr}),and_v(v:after({lock_height}),pk({b_pub_keyexpr}))}})"
    );
    let shared_sign_b_desc = format!(
        "tr({internal_pub_keyexpr},{{multi_a(2,{a_pub_keyexpr},{b_priv_keyexpr}),and_v(v:after({lock_height}),pk({b_priv_keyexpr}))}})"
    );

    let mut wallet_shared_watch = Wallet::create_single(shared_watch_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("shared watch descriptor should be valid");
    let wallet_shared_sign_a = Wallet::create_single(shared_sign_a_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("shared signer A descriptor should be valid");
    let wallet_shared_sign_b = Wallet::create_single(shared_sign_b_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("shared signer B descriptor should be valid");

    let shared_address = wallet_shared_watch
        .reveal_next_address(KeychainKind::External)
        .address
        .to_string();

    println!("[step2] send 2 BTC to A with rust-built PSBT");
    let (miner_utxo_txid, miner_utxo_vout, miner_utxo_sats) = bitcoind.scan_address_utxo(&miner_address).await;
    let a_spk = Address::from_str(&a_address)
        .expect("valid A address")
        .require_network(Network::Regtest)
        .expect("A address should be regtest")
        .script_pubkey();
    let miner_spk = Address::from_str(&miner_address)
        .expect("valid miner address")
        .require_network(Network::Regtest)
        .expect("miner address should be regtest")
        .script_pubkey();

    let step2_change = miner_utxo_sats
        .checked_sub(FUND_TO_A_SATS + FEE_SATS)
        .expect("miner utxo should cover send + fee");

    let mut step2_psbt = build_single_input_two_output_psbt(
        &miner_utxo_txid,
        miner_utxo_vout,
        miner_utxo_sats,
        miner_spk.clone(),
        a_spk,
        FUND_TO_A_SATS,
        miner_spk,
        step2_change,
        Sequence::ENABLE_RBF_NO_LOCKTIME,
        absolute::LockTime::ZERO,
    );
    print_tx_report("ab-or-b-after-t/step2/unsigned", &step2_psbt.unsigned_tx);
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
    print_tx_report("ab-or-b-after-t/step2/final", &step2_tx);
    let step2_hex = serialize_hex(&step2_tx);
    let _step2_txid = bitcoind.broadcast_raw_tx(&step2_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    println!("[step3] shared address created in rust: {shared_address}");

    // Fund #1 for cooperative path with change back to A for second fund.
    println!("[step4] A funds shared output #1 (with change)");
    let (a_utxo1_txid, a_utxo1_vout, a_utxo1_sats) = bitcoind.scan_address_utxo(&a_address).await;
    let a_input_spk = Address::from_str(&a_address)
        .expect("valid A address")
        .require_network(Network::Regtest)
        .expect("A address should be regtest")
        .script_pubkey();
    let shared_spk = Address::from_str(&shared_address)
        .expect("valid shared address")
        .require_network(Network::Regtest)
        .expect("shared address should be regtest")
        .script_pubkey();
    let a_change_spk = a_input_spk.clone();

    let a_change_after_fund1 = a_utxo1_sats
        .checked_sub(FUND_SHARED_SATS + FEE_SATS)
        .expect("A utxo should cover first shared fund + fee");

    let mut fund1_psbt = build_single_input_two_output_psbt(
        &a_utxo1_txid,
        a_utxo1_vout,
        a_utxo1_sats,
        a_input_spk,
        shared_spk.clone(),
        FUND_SHARED_SATS,
        a_change_spk,
        a_change_after_fund1,
        Sequence::ENABLE_RBF_NO_LOCKTIME,
        absolute::LockTime::ZERO,
    );
    print_tx_report("ab-or-b-after-t/fund1/unsigned", &fund1_psbt.unsigned_tx);
    let fund1_finalized = wallet_a_single
        .sign(
            &mut fund1_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: true,
                ..Default::default()
            },
        )
        .expect("A fund1 signing should succeed");
    assert!(fund1_finalized, "A fund1 should finalize");
    let fund1_tx = fund1_psbt.extract_tx().expect("fund1 extract tx");
    print_tx_report("ab-or-b-after-t/fund1/final", &fund1_tx);
    let fund1_hex = serialize_hex(&fund1_tx);
    let _fund1_txid = bitcoind.broadcast_raw_tx(&fund1_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    println!("[step5] cooperative path A+B spends shared output #1 to B");
    let (shared1_txid, shared1_vout, shared1_sats) = bitcoind.scan_address_utxo(&shared_address).await;
    let b_spk = Address::from_str(&b_address)
        .expect("valid B address")
        .require_network(Network::Regtest)
        .expect("B address should be regtest")
        .script_pubkey();
    let coop_to_b = shared1_sats.checked_sub(FEE_SATS).expect("shared1 should cover fee");

    let mut coop_psbt = build_single_input_one_output_psbt(
        &shared1_txid,
        shared1_vout,
        shared1_sats,
        shared_spk.clone(),
        b_spk.clone(),
        coop_to_b,
        Sequence::ENABLE_RBF_NO_LOCKTIME,
        absolute::LockTime::ZERO,
    );
    print_tx_report("ab-or-b-after-t/coop/unsigned", &coop_psbt.unsigned_tx);

    let a_only = wallet_shared_sign_a
        .sign(
            &mut coop_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: false,
                ..Default::default()
            },
        )
        .expect("A partial coop signing should succeed");
    assert!(!a_only, "A-only should not finalize cooperative path");

    let coop_finalized = wallet_shared_sign_b
        .sign(
            &mut coop_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: true,
                ..Default::default()
            },
        )
        .expect("B completes cooperative signing");
    assert!(coop_finalized, "A+B should finalize cooperative path");

    let coop_tx = coop_psbt.extract_tx().expect("coop extract tx");
    print_tx_report("ab-or-b-after-t/coop/final", &coop_tx);
    let coop_hex = serialize_hex(&coop_tx);
    let _coop_txid = bitcoind.broadcast_raw_tx(&coop_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    println!("[step6] fund shared output #2 from A change for timeout path");
    let (a_utxo2_txid, a_utxo2_vout, a_utxo2_sats) = bitcoind.scan_address_utxo(&a_address).await;
    let mut fund2_psbt = build_single_input_one_output_psbt(
        &a_utxo2_txid,
        a_utxo2_vout,
        a_utxo2_sats,
        Address::from_str(&a_address)
            .expect("valid A address")
            .require_network(Network::Regtest)
            .expect("A address should be regtest")
            .script_pubkey(),
        shared_spk.clone(),
        FUND_SHARED_SATS,
        Sequence::ENABLE_RBF_NO_LOCKTIME,
        absolute::LockTime::ZERO,
    );
    print_tx_report("ab-or-b-after-t/fund2/unsigned", &fund2_psbt.unsigned_tx);
    let fund2_finalized = wallet_a_single
        .sign(
            &mut fund2_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: true,
                ..Default::default()
            },
        )
        .expect("A fund2 signing should succeed");
    assert!(fund2_finalized, "A fund2 should finalize");
    let fund2_tx = fund2_psbt.extract_tx().expect("fund2 extract tx");
    print_tx_report("ab-or-b-after-t/fund2/final", &fund2_tx);
    let fund2_hex = serialize_hex(&fund2_tx);
    let _fund2_txid = bitcoind.broadcast_raw_tx(&fund2_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    println!("[step7] timeout path B+t spend: pre-timeout fail, post-timeout succeed");
    let (shared2_txid, shared2_vout, shared2_sats) = bitcoind.scan_address_utxo(&shared_address).await;
    let timeout_to_b = shared2_sats.checked_sub(FEE_SATS).expect("shared2 should cover fee");

    let lock_time = absolute::LockTime::from_consensus(lock_height);
    let mut timeout_psbt = build_single_input_one_output_psbt(
        &shared2_txid,
        shared2_vout,
        shared2_sats,
        shared_spk,
        b_spk,
        timeout_to_b,
        Sequence::ENABLE_RBF_NO_LOCKTIME,
        lock_time,
    );
    print_tx_report("ab-or-b-after-t/timeout/unsigned", &timeout_psbt.unsigned_tx);

    let timeout_finalized = wallet_shared_sign_b
        .sign(
            &mut timeout_psbt,
            SignOptions {
                trust_witness_utxo: true,
                try_finalize: true,
                ..Default::default()
            },
        )
        .expect("B timeout signing should succeed");
    assert!(timeout_finalized, "B should finalize timeout path");

    let timeout_tx = timeout_psbt.extract_tx().expect("timeout extract tx");
    print_tx_report("ab-or-b-after-t/timeout/final", &timeout_tx);
    let timeout_hex = serialize_hex(&timeout_tx);

    let pre = bitcoind.try_broadcast_raw_tx(&timeout_hex).await;
    assert!(pre.is_err(), "pre-timeout broadcast must fail for CLTV path");

    let current = bitcoind.get_block_count().await as u32;
    if current < lock_height {
        let to_mine = (lock_height - current) as u64;
        bitcoind.mine_blocks(to_mine, &miner_address).await;
    }

    let timeout_txid = bitcoind.broadcast_raw_tx(&timeout_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    println!("[step8] verify B got both cooperative and timeout payouts");
    let b_total = bitcoind.scan_address_total_sats(&b_address).await;
    let expected_b = coop_to_b + timeout_to_b;
    assert_eq!(b_total, expected_b, "B should receive both payouts");

    let verbose = bitcoind.get_raw_transaction_verbose(&timeout_txid).await;
    let confirmations = verbose["confirmations"].as_u64().unwrap_or(0);
    assert!(confirmations >= 1, "timeout tx must confirm");

    println!("[done] validated both branches: A+B and B+t");
}
