use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::Duration;

use nostr_sdk::prelude::Event;
use openark_rs::{
    NewRoundJoinKeys, NewRoundJoinOffboard, NewRoundJoinOnboard, NewRoundJoinPayload,
    NewRoundStartPreparedPayload, NewRoundVtxoTreeAcceptPayload,
    NewRoundVtxoTreeProposalPayload, NewRoundVtxoTreeProposalRecycle,
};
use serde_json::{Value, json};
use testcontainers::{
    ContainerAsync,
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use testcontainers::ImageExt;

pub async fn start_relay() -> (testcontainers::ContainerAsync<GenericImage>, String) {
    println!("[relay] starting strfry test container");
    let container = GenericImage::new("strfry-strfry", "latest")
        .with_exposed_port(7777.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Started websocket server"))
        .start()
        .await
        .expect("Failed to start strfry container");

    let host_port = container
        .get_host_port_ipv4(7777)
        .await
        .expect("Failed to map strfry port");

    println!("[relay] strfry ready on ws://localhost:{host_port}");
    (container, format!("ws://localhost:{host_port}"))
}

#[allow(dead_code)]
pub fn canonical_join_payload(actor: &str, onboard_outpoint: String) -> NewRoundJoinPayload {
    // Onboarding outpoints are runtime-prepared, confirmed 1 BTC UTXOs.
    NewRoundJoinPayload {
        onboard: vec![NewRoundJoinOnboard {
            outpoint: onboard_outpoint,
        }],
        offboard: Vec::<NewRoundJoinOffboard>::new(),
        keys: NewRoundJoinKeys {
            user_pubkey: format!("{actor}-user-pubkey"),
            agent_pubkey: format!("{actor}-agent-pubkey"),
        },
        address_of_last_resort: None,
    }
}

pub const PROPOSAL_VTXO_OUTPOINT_0: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:0";
pub const PROPOSAL_VTXO_OUTPOINT_1: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb:1";
pub const PREPARE_FORFEIT_OUTPOINT_0: &str =
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd:0";
pub const PREPARE_FORFEIT_OUTPOINT_1: &str =
    "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee:1";
pub const PREPARE_FUNDING_OUTPOINT_0: &str =
    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff:0";

#[allow(dead_code)]
pub fn canonical_vtxo_tree_proposal_payload() -> NewRoundVtxoTreeProposalPayload {
    NewRoundVtxoTreeProposalPayload {
        root_tx: "BASE64_ROOT_TX".to_string(),
        vtxo_tree: vec![
            "BASE64_VTXO_TX_0".to_string(),
            "BASE64_VTXO_TX_1".to_string(),
        ],
        forfeit_tree: vec![
            "BASE64_FORFEIT_TX_0".to_string(),
            "BASE64_FORFEIT_TX_1".to_string(),
        ],
        recycle: NewRoundVtxoTreeProposalRecycle {
            unilateral_tx: "BASE64_RECYCLE_UNILATERAL_TX".to_string(),
            cooperative_tx: "BASE64_RECYCLE_COOPERATIVE_TX".to_string(),
        },
    }
}

#[allow(dead_code)]
pub fn canonical_vtxo_tree_accept_payload(actor: &str) -> NewRoundVtxoTreeAcceptPayload {
    let mut signatures = std::collections::BTreeMap::new();
    signatures.insert(
        PROPOSAL_VTXO_OUTPOINT_0.to_string(),
        format!("BASE64_SIG_V0_{actor}"),
    );
    signatures.insert(
        PROPOSAL_VTXO_OUTPOINT_1.to_string(),
        format!("BASE64_SIG_V1_{actor}"),
    );
    NewRoundVtxoTreeAcceptPayload {
        vtxo_tree_signatures: signatures,
    }
}

#[allow(dead_code)]
pub fn canonical_start_prepared_payload(actor: &str) -> NewRoundStartPreparedPayload {
    let mut funding_signatures = std::collections::BTreeMap::new();
    funding_signatures.insert(
        PREPARE_FUNDING_OUTPOINT_0.to_string(),
        format!("BASE64_FUNDING_SIG_{actor}"),
    );
    let mut forfeit_signatures = std::collections::BTreeMap::new();
    forfeit_signatures.insert(
        PREPARE_FORFEIT_OUTPOINT_0.to_string(),
        format!("BASE64_FORFEIT_SIG_0_{actor}"),
    );
    forfeit_signatures.insert(
        PREPARE_FORFEIT_OUTPOINT_1.to_string(),
        format!("BASE64_FORFEIT_SIG_1_{actor}"),
    );

    NewRoundStartPreparedPayload {
        funding_signatures,
        forfeit_signatures,
        recycle_signature: format!("BASE64_RECYCLE_SIG_{actor}"),
    }
}

pub struct TraceRecorder {
    test_name: String,
    round_id: Option<String>,
    events: Vec<Value>,
    notes: Vec<Value>,
    started_ms: u128,
}

#[allow(dead_code)]
impl TraceRecorder {
    pub fn new(test_name: &str, round_id: Option<&str>) -> Self {
        Self {
            test_name: test_name.to_string(),
            round_id: round_id.map(ToOwned::to_owned),
            events: Vec::new(),
            notes: Vec::new(),
            started_ms: now_ms(),
        }
    }

    pub fn note(&mut self, phase: &str, message: &str) {
        let entry = json!({
            "ts_ms": now_ms(),
            "phase": phase,
            "message": message,
        });
        self.notes.push(entry.clone());
        println!("[trace] {}", entry);
    }

    pub fn event(&mut self, actor: &str, label: &str, event: &Event) {
        let entry = json!({
            "ts_ms": now_ms(),
            "actor": actor,
            "label": label,
            "id": event.id.to_string(),
            "kind": event.kind.as_u16(),
            "pubkey": event.pubkey.to_string(),
            "tags": event.tags,
            "content": event.content,
        });
        self.events.push(entry.clone());
        println!("[trace] {}", entry);
    }

    pub fn finish(self, status: &str, extra: Value) {
        let ended_ms = now_ms();
        let summary = json!({
            "test": self.test_name,
            "round_id": self.round_id,
            "status": status,
            "started_ms": self.started_ms,
            "ended_ms": ended_ms,
            "duration_ms": ended_ms.saturating_sub(self.started_ms),
            "event_count": self.events.len(),
            "note_count": self.notes.len(),
            "extra": extra,
        });

        println!("[summary] {}", summary);

        if let Some(dir) = trace_dir() {
            let _ = fs::create_dir_all(&dir);
            let stamp = ended_ms;
            let stem = format!("{}-{}", self.test_name, stamp);

            let mut events_path = PathBuf::from(&dir);
            events_path.push(format!("{}.events.jsonl", stem));

            let mut summary_path = PathBuf::from(&dir);
            summary_path.push(format!("{}.summary.json", stem));

            if let Ok(mut file) = File::create(events_path) {
                for e in &self.events {
                    let _ = writeln!(file, "{}", e);
                }
                for n in &self.notes {
                    let _ = writeln!(file, "{}", n);
                }
            }

            let _ = fs::write(summary_path, summary.to_string());
        }
    }
}

fn trace_dir() -> Option<String> {
    env::var("OPENARK_TRACE_DIR").ok().filter(|s| !s.trim().is_empty())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

const BTC_RPC_USER: &str = "rpcuser";
const BTC_RPC_PASS: &str = "rpcpass";
const BTC_RPC_PORT: u16 = 18443;

pub struct RegtestFundingHarness {
    _container: ContainerAsync<GenericImage>,
    rpc_url: String,
    client: reqwest::Client,
}

impl RegtestFundingHarness {
    pub async fn start() -> Self {
        let container = GenericImage::new("ruimarinho/bitcoin-core", "latest")
            .with_exposed_port(BTC_RPC_PORT.tcp())
            .with_cmd(vec![
                "-regtest=1".to_string(),
                "-server=1".to_string(),
                "-txindex=1".to_string(),
                "-printtoconsole".to_string(),
                "-fallbackfee=0.0002".to_string(),
                "-rpcbind=0.0.0.0".to_string(),
                "-rpcallowip=0.0.0.0/0".to_string(),
                format!("-rpcuser={BTC_RPC_USER}"),
                format!("-rpcpassword={BTC_RPC_PASS}"),
            ])
            .start()
            .await
            .expect("Failed to start bitcoind container");

        let host_port = container
            .get_host_port_ipv4(BTC_RPC_PORT)
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

    async fn get_new_miner_address(&self) -> String {
        self.rpc_call_wallet("miner", "getnewaddress", json!(["", "bech32m"]))
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

    async fn send_to_address(&self, address: &str, amount_btc: f64) {
        self.rpc_call_wallet("miner", "sendtoaddress", json!([address, amount_btc]))
            .await
            .expect("sendtoaddress should succeed");
    }

    async fn send_to_address_with_txid(&self, address: &str, amount_btc: f64) -> String {
        self.rpc_call_wallet("miner", "sendtoaddress", json!([address, amount_btc]))
            .await
            .expect("sendtoaddress should succeed")
            .as_str()
            .expect("sendtoaddress should return txid string")
            .to_string()
    }

    async fn scan_address_utxo(&self, address: &str) -> (String, u64, u64) {
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
        let sats = (unspent["amount"]
            .as_f64()
            .expect("amount should be f64")
            * 100_000_000.0)
            .round() as u64;
        (txid, vout, sats)
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
        (total_btc * 100_000_000.0).round() as u64
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
            .basic_auth(BTC_RPC_USER, Some(BTC_RPC_PASS))
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

pub async fn fund_wallets_two_btc(wallets: &[(&str, String, String)]) {
    let mut seen_seeds = HashSet::new();
    for (name, _addr, seed) in wallets {
        let inserted = seen_seeds.insert(seed.clone());
        assert!(inserted, "duplicate seed phrase detected for actor {name}");
    }

    let harness = RegtestFundingHarness::start().await;
    let miner_address = harness.get_new_miner_address().await;
    harness.mine_blocks(101, &miner_address).await;

    for (name, address, _) in wallets {
        println!("[fund] actor={name} address={address} target=2.0 BTC");
        harness.send_to_address(address, 2.0).await;
    }
    harness.mine_blocks(1, &miner_address).await;

    for (name, address, _) in wallets {
        let total_sats = harness.scan_address_total_sats(address).await;
        println!("[fund] actor={name} confirmed_sats={total_sats}");
        assert_eq!(
            total_sats, 200_000_000,
            "actor {name} should have exactly 2 BTC funded"
        );
    }
}

pub struct PreparedOnboardUtxo {
    pub outpoint: String,
    pub amount_sats: u64,
}

pub async fn prepare_onboard_utxos_one_btc(
    trace: &mut TraceRecorder,
    wallets: &[(&str, String, String)],
) -> BTreeMap<String, PreparedOnboardUtxo> {
    // Each actor receives a dedicated, confirmed 1 BTC UTXO for onboarding.
    let mut seen_seeds = HashSet::new();
    for (name, _addr, seed) in wallets {
        let inserted = seen_seeds.insert(seed.clone());
        assert!(inserted, "duplicate seed phrase detected for actor {name}");
    }

    let harness = RegtestFundingHarness::start().await;
    let miner_address = harness.get_new_miner_address().await;
    harness.mine_blocks(101, &miner_address).await;

    for (name, address, _) in wallets {
        let txid = harness.send_to_address_with_txid(address, 1.0).await;
        trace.note(
            "onboarding_prepare",
            &format!("actor={name} address={address} funding_txid={txid} target_sats=100000000"),
        );
    }
    harness.mine_blocks(1, &miner_address).await;

    let mut prepared = BTreeMap::new();
    for (name, address, _) in wallets {
        let (txid, vout, sats) = harness.scan_address_utxo(address).await;
        assert_eq!(
            sats, 100_000_000,
            "prepared onboarding utxo for actor {name} must be exactly 1 BTC"
        );
        let outpoint = format!("{txid}:{vout}");
        trace.note(
            "onboarding_prepared",
            &format!("actor={name} outpoint={outpoint} amount_sats={sats}"),
        );
        prepared.insert(
            (*name).to_string(),
            PreparedOnboardUtxo {
                outpoint,
                amount_sats: sats,
            },
        );
    }

    prepared
}
