use std::collections::BTreeMap;
use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use nostr_sdk::prelude::Event;
use openark_rs::{
    NewRoundJoinKeys, NewRoundJoinOffboard, NewRoundJoinOnboard, NewRoundJoinPayload,
    NewRoundStartPreparedPayload, NewRoundVtxoTreeAcceptPayload, NewRoundVtxoTreeProposalPayload,
    Step3ProposalBuildInput, build_step3_proposal_payload,
};
use serde_json::{Value, json};
use testcontainers::ImageExt;
use testcontainers::{
    ContainerAsync, GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

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
pub fn canonical_join_payload(
    actor: &str,
    user_pubkey: String,
    onboard_outpoint: String,
) -> NewRoundJoinPayload {
    // Onboarding outpoints are runtime-prepared, confirmed 1 BTC UTXOs.
    NewRoundJoinPayload {
        onboard: vec![NewRoundJoinOnboard {
            outpoint: onboard_outpoint,
        }],
        offboard: Vec::<NewRoundJoinOffboard>::new(),
        keys: NewRoundJoinKeys {
            user_pubkey,
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
    let input = Step3ProposalBuildInput {
        round_label: "r-step3-sim".to_string(),
        participants: vec![
            "Alice".to_string(),
            "Bob".to_string(),
            "Carol".to_string(),
            "Dave".to_string(),
            "Eve".to_string(),
        ],
        onboard_outpoints: vec![
            PROPOSAL_VTXO_OUTPOINT_0.to_string(),
            PROPOSAL_VTXO_OUTPOINT_1.to_string(),
        ],
        closing_block: 910_000,
        recycled_block: 910_144,
        leaf_timeout: 10,
    };
    build_step3_proposal_payload(&input)
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
    env::var("OPENARK_TRACE_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
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

#[allow(dead_code)]
struct ScannedAddressUtxo {
    txid: String,
    vout: u64,
    amount_sats: u64,
    confirmations: u64,
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

    pub async fn get_new_miner_address(&self) -> String {
        self.rpc_call_wallet("miner", "getnewaddress", json!(["", "bech32m"]))
            .await
            .expect("getnewaddress should succeed")
            .as_str()
            .expect("address should be string")
            .to_string()
    }

    pub async fn mine_blocks(&self, blocks: u64, address: &str) {
        self.rpc_call_wallet("miner", "generatetoaddress", json!([blocks, address]))
            .await
            .expect("generatetoaddress should succeed");
    }

    pub async fn broadcast_raw_tx(&self, hex: &str) -> String {
        self.rpc_call("sendrawtransaction", json!([hex]))
            .await
            .expect("sendrawtransaction should succeed")
            .as_str()
            .expect("txid should be string")
            .to_string()
    }

    pub async fn try_broadcast_raw_tx(&self, hex: &str) -> Result<String, String> {
        self.rpc_call("sendrawtransaction", json!([hex]))
            .await
            .map(|v| v.as_str().unwrap_or_default().to_string())
    }

    pub async fn send_to_address(&self, address: &str, amount_btc: f64) {
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

    pub async fn scan_address_utxo(&self, address: &str) -> (String, u64, u64) {
        let unspent = self
            .scan_address_utxos(address)
            .await
            .into_iter()
            .next()
            .expect("expected at least one unspent for address");
        (unspent.txid, unspent.vout, unspent.amount_sats)
    }

    async fn scan_address_utxos(&self, address: &str) -> Vec<ScannedAddressUtxo> {
        let descriptor = format!("addr({address})");
        let result = self
            .rpc_call("scantxoutset", json!(["start", [descriptor]]))
            .await
            .expect("scantxoutset should succeed");
        let tip_height = self
            .rpc_call("getblockcount", json!([]))
            .await
            .expect("getblockcount should succeed")
            .as_u64()
            .expect("getblockcount should return u64");
        let unspents = result["unspents"]
            .as_array()
            .expect("unspents should be array");
        let mut utxos = Vec::new();
        for unspent in unspents {
            let txid = unspent["txid"]
                .as_str()
                .expect("txid should be string")
                .to_string();
            let vout = unspent["vout"].as_u64().expect("vout should be u64");
            let amount_sats = (unspent["amount"].as_f64().expect("amount should be f64")
                * 100_000_000.0)
                .round() as u64;
            let height = unspent["height"].as_u64().unwrap_or(0);
            let confirmations = if height == 0 {
                0
            } else {
                tip_height.saturating_sub(height) + 1
            };
            utxos.push(ScannedAddressUtxo {
                txid,
                vout,
                amount_sats,
                confirmations,
            });
        }
        utxos.sort_by(|a, b| a.txid.cmp(&b.txid).then(a.vout.cmp(&b.vout)));
        utxos
    }

    pub async fn scan_address_total_sats(&self, address: &str) -> u64 {
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

    async fn rpc_call_wallet(
        &self,
        wallet: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
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
            return Err(format!(
                "RPC HTTP status error for {method}: {status}, body: {json}"
            ));
        }
        if !json["error"].is_null() {
            return Err(format!(
                "RPC returned error for {method}: {}",
                json["error"]
            ));
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

#[allow(dead_code)]
pub struct PreparedAspFundingUtxo {
    pub outpoint: String,
    pub value_sats: u64,
    pub confirmations: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AspFundingPoolState {
    Available,
    Reserved,
    Spent,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AspFundingPoolEntry {
    pub outpoint: String,
    pub value_sats: u64,
    pub confirmations: u64,
    pub state: AspFundingPoolState,
}

#[allow(dead_code)]
pub struct AspFundingPool {
    entries: BTreeMap<String, AspFundingPoolEntry>,
    min_confirmations_required: u64,
}

#[allow(dead_code)]
pub const ASP_POOL_MIN_UTXO_COUNT: usize = 8;
#[allow(dead_code)]
pub const ASP_POOL_TARGET_UTXO_VALUE_SATS: u64 = 10_000_000;
#[allow(dead_code)]
pub const ASP_POOL_MIN_CONFIRMATIONS: u64 = 1;
#[allow(dead_code)]
pub const ASP_TREE_CONSTRUCTION_FEERATE_SAT_PER_VB: u64 = 1;
#[allow(dead_code)]
pub const ASP_TREE_PROPOSAL_WORST_CASE_BASE_VBYTES: u64 = 300;
#[allow(dead_code)]
pub const ASP_TREE_PROPOSAL_WORST_CASE_PER_PARTICIPANT_VBYTES: u64 = 200;

#[allow(dead_code)]
impl AspFundingPool {
    pub fn new(utxos: Vec<PreparedAspFundingUtxo>) -> Self {
        Self::new_with_min_confirmations(utxos, ASP_POOL_MIN_CONFIRMATIONS)
    }

    pub fn new_with_min_confirmations(
        utxos: Vec<PreparedAspFundingUtxo>,
        min_confirmations_required: u64,
    ) -> Self {
        let mut entries = BTreeMap::new();
        for utxo in utxos {
            let duplicate = entries.contains_key(&utxo.outpoint);
            assert!(!duplicate, "duplicate pool outpoint {}", utxo.outpoint);
            entries.insert(
                utxo.outpoint.clone(),
                AspFundingPoolEntry {
                    outpoint: utxo.outpoint,
                    value_sats: utxo.value_sats,
                    confirmations: utxo.confirmations,
                    state: AspFundingPoolState::Available,
                },
            );
        }
        Self {
            entries,
            min_confirmations_required,
        }
    }

    pub fn available_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.state == AspFundingPoolState::Available)
            .count()
    }

    pub fn reserved_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.state == AspFundingPoolState::Reserved)
            .count()
    }

    pub fn spent_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.state == AspFundingPoolState::Spent)
            .count()
    }

    pub fn available_value_sats(&self) -> u64 {
        self.entries
            .values()
            .filter(|entry| entry.state == AspFundingPoolState::Available)
            .map(|entry| entry.value_sats)
            .sum()
    }

    pub fn reserved_value_sats(&self) -> u64 {
        self.entries
            .values()
            .filter(|entry| entry.state == AspFundingPoolState::Reserved)
            .map(|entry| entry.value_sats)
            .sum()
    }

    pub fn spent_value_sats(&self) -> u64 {
        self.entries
            .values()
            .filter(|entry| entry.state == AspFundingPoolState::Spent)
            .map(|entry| entry.value_sats)
            .sum()
    }

    pub fn reserve_for_amount(&mut self, required_sats: u64) -> Vec<String> {
        assert!(
            required_sats > 0,
            "required_sats must be greater than zero when reserving pool UTXOs"
        );

        let mut candidates: Vec<&AspFundingPoolEntry> = self
            .entries
            .values()
            .filter(|entry| {
                entry.state == AspFundingPoolState::Available
                    && entry.confirmations >= self.min_confirmations_required
            })
            .collect();
        candidates.sort_by(|a, b| {
            a.confirmations
                .cmp(&b.confirmations)
                .then(a.outpoint.cmp(&b.outpoint))
        });

        let mut selected = Vec::new();
        let mut selected_total = 0u64;
        for candidate in candidates {
            selected_total = selected_total.saturating_add(candidate.value_sats);
            selected.push(candidate.outpoint.clone());
            if selected_total >= required_sats {
                break;
            }
        }

        assert!(
            selected_total >= required_sats,
            "insufficient available pool balance: required {}, selected {}",
            required_sats,
            selected_total
        );

        for outpoint in &selected {
            let entry = self
                .entries
                .get_mut(outpoint)
                .expect("selected outpoint must exist");
            assert_eq!(
                entry.state,
                AspFundingPoolState::Available,
                "outpoint {} must be available when reserving",
                outpoint
            );
            entry.state = AspFundingPoolState::Reserved;
        }

        selected
    }

    pub fn release(&mut self, outpoints: &[String]) {
        for outpoint in outpoints {
            let entry = self
                .entries
                .get_mut(outpoint)
                .unwrap_or_else(|| panic!("unknown pool outpoint {}", outpoint));
            assert_eq!(
                entry.state,
                AspFundingPoolState::Reserved,
                "outpoint {} must be reserved to release",
                outpoint
            );
            entry.state = AspFundingPoolState::Available;
        }
    }

    pub fn consume(&mut self, outpoints: &[String]) {
        for outpoint in outpoints {
            let entry = self
                .entries
                .get_mut(outpoint)
                .unwrap_or_else(|| panic!("unknown pool outpoint {}", outpoint));
            assert_eq!(
                entry.state,
                AspFundingPoolState::Reserved,
                "outpoint {} must be reserved to consume",
                outpoint
            );
            entry.state = AspFundingPoolState::Spent;
        }
    }
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

#[allow(dead_code)]
pub async fn initialize_default_asp_funding_pool(
    trace: &mut TraceRecorder,
    asp_name: &str,
    asp_address: &str,
) -> AspFundingPool {
    let prepared = prepare_asp_funding_pool_utxos(
        trace,
        asp_address,
        ASP_POOL_MIN_UTXO_COUNT,
        ASP_POOL_TARGET_UTXO_VALUE_SATS,
        ASP_POOL_MIN_CONFIRMATIONS,
    )
    .await;
    let pool = AspFundingPool::new(prepared);
    assert!(
        pool.available_count() >= ASP_POOL_MIN_UTXO_COUNT,
        "ASP pool for {asp_name} must have at least {} available UTXOs",
        ASP_POOL_MIN_UTXO_COUNT
    );
    trace.note(
        "asp_pool_snapshot",
        &format!(
            "actor={asp_name} available_count={} reserved_count={} spent_count={} available_value_sats={}",
            pool.available_count(),
            pool.reserved_count(),
            pool.spent_count(),
            pool.available_value_sats()
        ),
    );
    println!(
        "[asp-pool] actor={asp_name} available={} reserved={} spent={} available_value_sats={}",
        pool.available_count(),
        pool.reserved_count(),
        pool.spent_count(),
        pool.available_value_sats()
    );
    pool
}

#[allow(dead_code)]
pub fn estimate_worst_case_tree_proposal_vbytes(participant_count: usize) -> u64 {
    ASP_TREE_PROPOSAL_WORST_CASE_BASE_VBYTES
        + ASP_TREE_PROPOSAL_WORST_CASE_PER_PARTICIPANT_VBYTES * participant_count as u64
}

#[allow(dead_code)]
pub fn estimate_tree_proposal_fee_budget_sats(participant_count: usize) -> u64 {
    estimate_worst_case_tree_proposal_vbytes(participant_count)
        * ASP_TREE_CONSTRUCTION_FEERATE_SAT_PER_VB
}

#[allow(dead_code)]
pub async fn prepare_asp_funding_pool_utxos(
    trace: &mut TraceRecorder,
    asp_address: &str,
    utxo_count: usize,
    target_value_sats: u64,
    min_confirmations: u64,
) -> Vec<PreparedAspFundingUtxo> {
    assert!(utxo_count > 0, "utxo_count must be greater than zero");
    assert!(
        target_value_sats > 0,
        "target_value_sats must be greater than zero"
    );

    let harness = RegtestFundingHarness::start().await;
    let miner_address = harness.get_new_miner_address().await;
    harness.mine_blocks(101, &miner_address).await;

    let target_value_btc = target_value_sats as f64 / 100_000_000.0;
    let mut funding_txids = Vec::with_capacity(utxo_count);
    for index in 0..utxo_count {
        let txid = harness
            .send_to_address_with_txid(asp_address, target_value_btc)
            .await;
        trace.note(
            "asp_pool_prepare",
            &format!(
                "index={index} address={asp_address} funding_txid={txid} target_sats={target_value_sats}"
            ),
        );
        funding_txids.push(txid);
    }

    harness
        .mine_blocks(min_confirmations.max(1), &miner_address)
        .await;

    let all_utxos = harness.scan_address_utxos(asp_address).await;
    let mut prepared = Vec::new();
    for utxo in all_utxos {
        if !funding_txids.contains(&utxo.txid) {
            continue;
        }
        assert!(
            utxo.confirmations >= min_confirmations,
            "funding utxo {}:{} must have at least {} confirmations",
            utxo.txid,
            utxo.vout,
            min_confirmations
        );
        assert_eq!(
            utxo.amount_sats, target_value_sats,
            "funding utxo {}:{} must match target value",
            utxo.txid, utxo.vout
        );
        let outpoint = format!("{}:{}", utxo.txid, utxo.vout);
        trace.note(
            "asp_pool_prepared",
            &format!(
                "outpoint={outpoint} value_sats={} confirmations={}",
                utxo.amount_sats, utxo.confirmations
            ),
        );
        prepared.push(PreparedAspFundingUtxo {
            outpoint,
            value_sats: utxo.amount_sats,
            confirmations: utxo.confirmations,
        });
    }

    prepared.sort_by(|a, b| a.outpoint.cmp(&b.outpoint));
    assert_eq!(
        prepared.len(),
        utxo_count,
        "expected {} prepared ASP pool UTXOs, got {}",
        utxo_count,
        prepared.len()
    );
    prepared
}
