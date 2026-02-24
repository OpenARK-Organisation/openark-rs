use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use nostr_sdk::prelude::Event;
use serde_json::{Value, json};
use testcontainers::{
    GenericImage,
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
