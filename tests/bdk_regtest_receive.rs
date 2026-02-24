use bdk_wallet::KeychainKind;
use openark_rs::create_regtest_bdk_wallet;
use serde_json::{Value, json};
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
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
        };

        harness.wait_until_ready().await;
        harness
            .rpc_call("createwallet", json!(["miner"]))
            .await
            .expect("createwallet should succeed");
        harness
    }

    async fn get_new_address(&self) -> String {
        self.rpc_call("getnewaddress", json!([]))
            .await
            .expect("getnewaddress should succeed")
            .as_str()
            .expect("getnewaddress should return string")
            .to_string()
    }

    async fn mine_blocks(&self, blocks: u64, address: &str) {
        self.rpc_call("generatetoaddress", json!([blocks, address]))
            .await
            .expect("generatetoaddress should succeed");
    }

    async fn send_to_address(&self, address: &str, amount_btc: f64) {
        self.rpc_call("sendtoaddress", json!([address, amount_btc]))
            .await
            .expect("sendtoaddress should succeed");
    }

    async fn scan_address_total(&self, address: &str) -> f64 {
        let descriptor = format!("addr({address})");
        let result = self
            .rpc_call("scantxoutset", json!(["start", [descriptor]]))
            .await
            .expect("scantxoutset should succeed");

        result["total_amount"]
            .as_f64()
            .expect("total_amount should be a number")
    }

    async fn wait_until_ready(&self) {
        for _ in 0..60 {
            if self.rpc_call("getblockchaininfo", json!([])).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        panic!("bitcoind RPC did not become ready in time");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn regtest_bitcoind_funds_bdk_wallet_with_one_btc() {
    let bitcoind = BitcoindHarness::start().await;

    let miner_address = bitcoind.get_new_address().await;
    bitcoind.mine_blocks(101, &miner_address).await;

    let mut wallet = create_regtest_bdk_wallet();
    let receive = wallet.reveal_next_address(KeychainKind::External);
    let receive_address = receive.address.to_string();

    bitcoind.send_to_address(&receive_address, 1.0).await;
    bitcoind.mine_blocks(1, &miner_address).await;

    let total = bitcoind.scan_address_total(&receive_address).await;
    assert!(
        (total - 1.0).abs() < 0.00000001,
        "expected 1.0 BTC at {receive_address}, got {total}"
    );
}
