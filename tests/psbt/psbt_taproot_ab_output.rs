use bdk_wallet::bitcoin::{
    bip32::{DerivationPath, Xpriv, Xpub},
    secp256k1::Secp256k1,
};
use serde_json::{Value, json};
use std::str::FromStr;
use std::time::Duration;
use testcontainers::{
    ContainerAsync, GenericImage,
    core::IntoContainerPort,
    runners::AsyncRunner,
};
use testcontainers::ImageExt;

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
        harness.create_wallet("a").await;
        harness.create_wallet("b").await;
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

    async fn import_descriptor(&self, wallet: &str, descriptor_with_checksum: &str) {
        let res = self
            .rpc_call_wallet(
                wallet,
                "importdescriptors",
                json!([[{
                    "desc": descriptor_with_checksum,
                    "timestamp": "now",
                    "active": false,
                    "internal": false,
                    "label": "ab-2of2-signer"
                }]]),
            )
            .await
            .expect("importdescriptors should succeed");
        let ok = res
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.get("success"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(ok, "descriptor import must succeed for wallet {wallet}: {res}");
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

    async fn wallet_process_psbt(
        &self,
        wallet: &str,
        psbt: &str,
        finalize: bool,
    ) -> (String, bool, Option<String>) {
        let result = self
            .rpc_call_wallet(
                wallet,
                "walletprocesspsbt",
                json!([psbt, true, "DEFAULT", true, finalize]),
            )
            .await
            .expect("walletprocesspsbt should succeed");

        let out_psbt = result["psbt"]
            .as_str()
            .expect("walletprocesspsbt.psbt should be string")
            .to_string();
        let complete = result["complete"].as_bool().unwrap_or(false);
        let hex = result["hex"].as_str().map(ToOwned::to_owned);
        (out_psbt, complete, hex)
    }

    async fn finalize_psbt(&self, psbt: &str) -> (bool, Option<String>) {
        let result = self
            .rpc_call("finalizepsbt", json!([psbt]))
            .await
            .expect("finalizepsbt should succeed");
        let complete = result["complete"].as_bool().unwrap_or(false);
        let hex = result["hex"].as_str().map(ToOwned::to_owned);
        (complete, hex)
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
async fn regtest_psbt_taproot_ab_output_requires_both_signatures() {
    let bitcoind = BitcoindHarness::start().await;

    println!("[setup] fund miner");
    let miner_address = bitcoind.get_new_address("miner", "bech32m").await;
    bitcoind.mine_blocks(101, &miner_address).await;

    let (a_priv_keyexpr, b_priv_keyexpr, a_pub_keyexpr, b_pub_keyexpr, internal_pub_keyexpr) = descriptor_key_exprs();
    println!("[setup] A keyexpr={a_pub_keyexpr}");
    println!("[setup] B keyexpr={b_pub_keyexpr}");

    // Watch descriptor defines the shared A+B taproot output.
    let watch_desc = format!("tr({internal_pub_keyexpr},multi_a(2,{a_pub_keyexpr},{b_pub_keyexpr}))");
    let watch_desc = bitcoind.get_descriptor_with_checksum(&watch_desc).await;
    let ab_address = bitcoind.derive_single_address(&watch_desc).await;
    println!("[phase1] A+B taproot address={ab_address}");

    // Signer-specific descriptors: each signer has only one private key.
    let a_sign_desc = format!("tr({internal_pub_keyexpr},multi_a(2,{a_priv_keyexpr},{b_pub_keyexpr}))");
    let b_sign_desc = format!("tr({internal_pub_keyexpr},multi_a(2,{a_pub_keyexpr},{b_priv_keyexpr}))");
    let a_sign_desc = bitcoind.get_descriptor_with_checksum(&a_sign_desc).await;
    let b_sign_desc = bitcoind.get_descriptor_with_checksum(&b_sign_desc).await;
    bitcoind.import_descriptor("a", &a_sign_desc).await;
    bitcoind.import_descriptor("b", &b_sign_desc).await;

    println!("[phase2] fund A+B output and confirm");
    let fund_txid = bitcoind.send_to_address("miner", &ab_address, 1.0).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    let (funded_txid, funded_vout, funded_amount) = bitcoind.scan_address_utxo(&ab_address).await;
    assert_eq!(funded_txid, fund_txid, "funded outpoint should match txid");
    assert!(funded_amount > 0.999, "expected ~1 BTC in A+B output");
    println!("[phase2] funded outpoint={funded_txid}:{funded_vout} amount={funded_amount} BTC");

    println!("[phase3] create and enrich PSBT");
    let destination = bitcoind.get_new_address("miner", "bech32m").await;
    let send_amount = funded_amount - 0.0002;
    let unsigned_psbt = bitcoind
        .create_psbt_one_input_one_output(&funded_txid, funded_vout, &destination, send_amount)
        .await;
    let enriched_psbt = bitcoind
        .utxo_update_psbt(&unsigned_psbt, std::slice::from_ref(&watch_desc))
        .await;

    println!("[phase4] A signs first; must be incomplete");
    let (a_psbt, a_complete, _) = bitcoind
        .wallet_process_psbt("a", &enriched_psbt, false)
        .await;
    assert!(!a_complete, "A-only signature must not complete 2-of-2 spend");
    let (a_finalizable, _) = bitcoind.finalize_psbt(&a_psbt).await;
    assert!(!a_finalizable, "A-only PSBT must not be finalizable");

    println!("[phase4] B-only sanity check; must be incomplete");
    let (b_psbt, b_complete, _) = bitcoind
        .wallet_process_psbt("b", &enriched_psbt, false)
        .await;
    assert!(!b_complete, "B-only signature must not complete 2-of-2 spend");
    let (b_finalizable, _) = bitcoind.finalize_psbt(&b_psbt).await;
    assert!(!b_finalizable, "B-only PSBT must not be finalizable");

    println!("[phase5] B signs A's PSBT; now complete");
    let (ab_psbt, _ab_complete, _) = bitcoind
        .wallet_process_psbt("b", &a_psbt, false)
        .await;
    let (ab_finalizable, _) = bitcoind.finalize_psbt(&ab_psbt).await;
    assert!(ab_finalizable, "A+B signatures should make PSBT finalizable");

    println!("[phase6] A finalizes");
    let (_, finalized_by_a, maybe_hex) = bitcoind
        .wallet_process_psbt("a", &ab_psbt, true)
        .await;
    assert!(finalized_by_a, "A finalization step should complete");

    let final_hex = if let Some(hex) = maybe_hex {
        hex
    } else {
        let (is_final, fallback_hex) = bitcoind.finalize_psbt(&ab_psbt).await;
        assert!(is_final, "finalizepsbt fallback must complete");
        fallback_hex.expect("final tx hex expected")
    };

    println!("[phase7] broadcast and confirm");
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

    println!("[done] A+B Taproot output was funded and spend required both A and B PSBT signatures");
}
