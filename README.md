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
