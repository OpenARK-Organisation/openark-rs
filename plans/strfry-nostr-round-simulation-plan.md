# Strfry Nostr Round Simulation Plan (openark-rs)

## Goal
Implement incremental OpenARK round-message simulation in `openark-rs` using Nostr `kind:150` events over a clean `strfry` relay.

Constraints for this phase:
- No Bitcoin transaction construction/signing.
- Use text placeholders (for example: `"VTXO_ROOT"`, `"VTXO_ROOT_SIGNATURE"`).
- Generic reusable code goes in `src`.
- Scenario/integration tests go in `tests`.

## Current Repository Findings
- Target repo: `/home/rene/git/openark-rs`
- Existing relay integration pattern uses `testcontainers` + Strfry startup.
- Codebase uses split modules for domain objects and transport helpers.

## Current Status

### Completed
1. Implemented generic `kind:150` round messaging support in `src`.
2. Implemented Step 1 test (`tests/round_step1_nostr_kind150.rs`) with strict assertions.
3. Added verbose test logging for Step 1 (`--nocapture` friendly).
4. Implemented Step 2 test (`tests/round_step2_two_actor_full_flow.rs`) with strict phase ordering.
5. Refactored objects into separate modules:
   - `src/actors/provider.rs`
   - `src/actors/user.rs`
   - `src/round/message.rs`
   - `src/round/transport.rs`
   - `src/round/matchers.rs`
   - `src/wallet.rs`
   - `src/lib.rs` as module wiring + re-exports.
6. Implemented Step 3 test (`tests/round_step3_multi_actor_full_flow.rs`):
   - multi-actor flow with Alice, Bob, Carol, Dave, Eve + Steve
   - per-phase completion counters
   - verbose actor-by-actor logging.
7. Implemented Step 4 failure-path tests (`tests/round_step4_failure_paths.rs`):
   - missing `new_round_join` blocks phase
   - invalid `new_round_vtxo_tree_accept` token blocks phase
   - invalid `new_round_start_prepared` token blocks activation
   - late `new_round_start_prepared` blocks activation timeout.
8. Stabilized Step 3 sequencing to remove intermittent race under full-suite execution.
9. Retested successfully:
   - Step 1, Step 2, Step 3, Step 4 tests
   - full `cargo test -- --nocapture` suite.
10. Implemented Step 5 repeatability/artifacts:
   - added shared test support in `tests/support.rs`
   - added `TraceRecorder` with structured `[trace]` and `[summary]` output
   - added optional JSON/JSONL artifact output via `OPENARK_TRACE_DIR`
   - refactored Step 1-4 tests to use shared support + structured summaries.
11. Retested successfully after Step 5:
   - full `cargo test --tests -- --nocapture` suite.

### Checkpoints Created
- `58e03e0` - Step 1 verbose two-actor kind:150 handshake.
- `66fde50` - Step 2 full two-actor kind:150 phase flow.
- `9f3f380` - module refactor + Step 3 multi-actor flow test.
- `e3f7e62` - plan progress update through Step 3.
- `f312081` - Step 4 failure-path tests + plan update.

### In Progress
- No active implementation in progress.

## Next Plan

### Step 6: Cleanup/Hardening
1. Reduce per-test `dead_code` warnings caused by shared `tests/support.rs`.
2. Decide CI/default behavior for `OPENARK_TRACE_DIR` artifact emission.
3. Optionally add a compact README snippet for running Step 1-5 tests locally.

## Definition of Done for Current Phase
- Step 1 through Step 4 passing in CI/local.
- Step 5 tracing/artifact layer implemented and passing.
- Shared objects split into separate files.
- Verbose, readable logs in integration tests.
- Full test suite green after refactor and failure-path coverage.
