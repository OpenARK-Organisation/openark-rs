use bdk_wallet::bitcoin::{
    Network,
    bip32::{DerivationPath, Xpriv, Xpub},
    consensus::encode::serialize_hex,
    psbt::Psbt,
    secp256k1::Secp256k1,
};
use bdk_wallet::{SignOptions, Wallet};
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
        harness.create_wallet("miner").await;
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

    async fn create_wallet(&self, name: &str) {
        self.rpc_call("createwallet", json!([name]))
            .await
            .unwrap_or_else(|e| panic!("createwallet {name} failed: {e}"));
    }

    async fn get_new_address(&self, wallet: &str, address_type: &str) -> String {
        self.rpc_call_wallet(wallet, "getnewaddress", json!(["", address_type]))
            .await
            .expect("getnewaddress should succeed")
            .as_str()
            .expect("address should be string")
            .to_string()
    }

    async fn mine_blocks(&self, blocks: u64, address: &str) {
        self.rpc_call_wallet("miner", "generatetoaddress", json!([blocks, address]))
            .await
            .expect("generatetoaddress should succeed");
    }

    async fn send_to_address(&self, wallet: &str, address: &str, amount_btc: f64) -> String {
        self.rpc_call_wallet(wallet, "sendtoaddress", json!([address, amount_btc]))
            .await
            .expect("sendtoaddress should succeed")
            .as_str()
            .expect("txid should be string")
            .to_string()
    }

    async fn get_descriptor_with_checksum(&self, descriptor: &str) -> String {
        let info = self
            .rpc_call("getdescriptorinfo", json!([descriptor]))
            .await
            .expect("getdescriptorinfo should succeed");
        let checksum = info["checksum"]
            .as_str()
            .expect("checksum should be string");
        format!("{descriptor}#{checksum}")
    }

    async fn derive_single_address(&self, descriptor_with_checksum: &str) -> String {
        self.rpc_call("deriveaddresses", json!([descriptor_with_checksum]))
            .await
            .expect("deriveaddresses should succeed")
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .expect("derived address expected")
            .to_string()
    }

    async fn scan_address_utxo(&self, address: &str) -> (String, u64, f64) {
        let descriptor = format!("addr({address})");
        let result = self
            .rpc_call("scantxoutset", json!(["start", [descriptor]]))
            .await
            .expect("scantxoutset should succeed");

        let unspent = result["unspents"]
            .as_array()
            .and_then(|u| u.first())
            .expect("expected at least one unspent for address");

        let txid = unspent["txid"]
            .as_str()
            .expect("txid should be string")
            .to_string();
        let vout = unspent["vout"].as_u64().expect("vout should be u64");
        let amount = unspent["amount"].as_f64().expect("amount should be f64");
        (txid, vout, amount)
    }

    async fn create_psbt_one_input_one_output(
        &self,
        txid: &str,
        vout: u64,
        destination: &str,
        amount_btc: f64,
    ) -> String {
        self.rpc_call(
            "createpsbt",
            json!([
                [{"txid": txid, "vout": vout}],
                [{destination: amount_btc}],
                0,
                true
            ]),
        )
        .await
        .expect("createpsbt should succeed")
        .as_str()
        .expect("psbt should be string")
        .to_string()
    }

    async fn utxo_update_psbt(&self, psbt: &str, descriptors: &[String]) -> String {
        self.rpc_call("utxoupdatepsbt", json!([psbt, descriptors]))
            .await
            .expect("utxoupdatepsbt should succeed")
            .as_str()
            .expect("updated psbt should be string")
            .to_string()
    }

    async fn broadcast_raw_tx(&self, hex: &str) -> String {
        self.rpc_call("sendrawtransaction", json!([hex]))
            .await
            .expect("sendrawtransaction should succeed")
            .as_str()
            .expect("txid should be string")
            .to_string()
    }

    async fn get_raw_transaction_verbose(&self, txid: &str) -> Value {
        self.rpc_call("getrawtransaction", json!([txid, true]))
            .await
            .expect("getrawtransaction should succeed")
    }

    async fn rpc_call_wallet(&self, wallet: &str, method: &str, params: Value) -> Result<Value, String> {
        let url = format!("{}/wallet/{wallet}", self.rpc_url);
        self.rpc_call_url(&url, method, params).await
    }

    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.rpc_call_url(&self.rpc_url, method, params).await
    }

    async fn rpc_call_url(&self, url: &str, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({
            "jsonrpc": "1.0",
            "id": "test",
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(url)
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

fn descriptor_key_exprs() -> (String, String, String, String, String) {
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

    (a_priv, b_priv, a_pub, b_pub, internal_pub)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn regtest_psbt_taproot_ab_output_rust_wallet_signers() {
    let bitcoind = BitcoindHarness::start().await;

    println!("[setup] fund miner");
    let miner_address = bitcoind.get_new_address("miner", "bech32m").await;
    bitcoind.mine_blocks(101, &miner_address).await;

    let (a_priv_keyexpr, b_priv_keyexpr, a_pub_keyexpr, b_pub_keyexpr, internal_pub_keyexpr) =
        descriptor_key_exprs();

    let watch_desc_raw =
        format!("tr({internal_pub_keyexpr},multi_a(2,{a_pub_keyexpr},{b_pub_keyexpr}))");
    let watch_desc = bitcoind.get_descriptor_with_checksum(&watch_desc_raw).await;
    let ab_address = bitcoind.derive_single_address(&watch_desc).await;
    println!("[phase1] A+B taproot address={ab_address}");

    let a_sign_desc = format!("tr({internal_pub_keyexpr},multi_a(2,{a_priv_keyexpr},{b_pub_keyexpr}))");
    let b_sign_desc = format!("tr({internal_pub_keyexpr},multi_a(2,{a_pub_keyexpr},{b_priv_keyexpr}))");

    let wallet_a = Wallet::create_single(a_sign_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("A signer wallet descriptor should be valid");
    let wallet_b = Wallet::create_single(b_sign_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("B signer wallet descriptor should be valid");

    println!("[phase2] fund A+B output and confirm");
    let fund_txid = bitcoind.send_to_address("miner", &ab_address, 1.0).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    let (funded_txid, funded_vout, funded_amount) = bitcoind.scan_address_utxo(&ab_address).await;
    assert_eq!(funded_txid, fund_txid, "funded outpoint should match txid");
    assert!(funded_amount > 0.999, "expected ~1 BTC in A+B output");
    println!(
        "[phase2] funded outpoint={funded_txid}:{funded_vout} amount={funded_amount} BTC"
    );

    println!("[phase3] create and enrich PSBT");
    let destination = bitcoind.get_new_address("miner", "bech32m").await;
    let send_amount = funded_amount - 0.0002;
    let unsigned_psbt = bitcoind
        .create_psbt_one_input_one_output(&funded_txid, funded_vout, &destination, send_amount)
        .await;
    let enriched_psbt = bitcoind
        .utxo_update_psbt(&unsigned_psbt, std::slice::from_ref(&watch_desc))
        .await;

    let sign_no_finalize = SignOptions {
        trust_witness_utxo: true,
        try_finalize: false,
        ..Default::default()
    };
    let sign_finalize = SignOptions {
        trust_witness_utxo: true,
        try_finalize: true,
        ..Default::default()
    };

    println!("[phase4] A-only signing must not finalize");
    let mut psbt_a_only = Psbt::from_str(&enriched_psbt).expect("valid base64 PSBT");
    let a_only_finalized = wallet_a
        .sign(&mut psbt_a_only, sign_finalize.clone())
        .expect("A signer should process PSBT");
    assert!(!a_only_finalized, "A-only signature must not finalize 2-of-2 spend");

    println!("[phase4] B-only signing must not finalize");
    let mut psbt_b_only = Psbt::from_str(&enriched_psbt).expect("valid base64 PSBT");
    let b_only_finalized = wallet_b
        .sign(&mut psbt_b_only, sign_finalize.clone())
        .expect("B signer should process PSBT");
    assert!(!b_only_finalized, "B-only signature must not finalize 2-of-2 spend");

    println!("[phase5] A then B signing must finalize");
    let mut psbt_ab = Psbt::from_str(&enriched_psbt).expect("valid base64 PSBT");
    let a_pass = wallet_a
        .sign(&mut psbt_ab, sign_no_finalize)
        .expect("A signer should add partial sig");
    assert!(!a_pass, "A partial signing should not finalize");

    let ab_finalized = wallet_b
        .sign(&mut psbt_ab, sign_finalize.clone())
        .expect("B signer should complete and finalize");
    assert!(ab_finalized, "A+B signing should finalize PSBT");

    let final_tx = psbt_ab
        .extract_tx()
        .expect("finalized PSBT should extract transaction");
    let final_hex = serialize_hex(&final_tx);

    println!("[phase6] broadcast and confirm");
    let spend_txid = bitcoind.broadcast_raw_tx(&final_hex).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    let spend_verbose = bitcoind.get_raw_transaction_verbose(&spend_txid).await;
    let confirmations = spend_verbose["confirmations"].as_u64().unwrap_or(0);
    assert!(confirmations >= 1, "spend tx must be confirmed");

    let vin_txid = spend_verbose["vin"]
        .as_array()
        .and_then(|vin| vin.first())
        .and_then(|vin0| vin0.get("txid"))
        .and_then(|txid| txid.as_str())
        .expect("spend tx vin[0].txid expected");
    assert_eq!(vin_txid, funded_txid, "spend must consume funded A+B output");

    println!("[done] Rust wallets signed the PSBT; bitcoind only provided chain services");
}
