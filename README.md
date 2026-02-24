# openark-rs
OpenArk implementation in Rust

## Nostr Round Simulation Tests
Run the Step 1-5 integration tests with verbose logs:

```bash
cargo test --tests -- --nocapture
```

Trace artifact output is opt-in. By default, tests only print `[trace]` and `[summary]` lines to stdout and do not write files. To emit JSON artifacts, set `OPENARK_TRACE_DIR`:

```bash
OPENARK_TRACE_DIR=./tmp/trace cargo test --tests -- --nocapture
```

When `OPENARK_TRACE_DIR` is set, each test writes:
- `<test>-<timestamp>.events.jsonl`
- `<test>-<timestamp>.summary.json`

## Phase 1 Canonical Message Contract
The Phase 1 round announcement flow is strict and uses canonical shapes:

- `new_round_initiate`
  - tags: `["t","new_round_initiate"]`, `["v","1.0.0"]`
  - content JSON: `{"closing_block": <u64>, "recycled_block": <u64>}`
- `new_round_join`
  - tags: `["t","new_round_join"]`, `["v","1.0.0"]`, `["e","<initiate_event_id>"]`
  - content JSON:
    - `onboard`: array of `{ "outpoint": "<txid:vout>" }`
    - `offboard`: array of `{ "address": "<bitcoin-address>" }`
    - `keys`: `{ "user_pubkey": "...", "agent_pubkey": "..." }`
    - optional `address_of_last_resort`

Notes:
- The initiate event id is the Phase 1 round reference.
- `new_round_join` content must be valid JSON and must not include `round_id`.
- Generic round-id send/wait paths are intentionally blocked for initiate/join.
