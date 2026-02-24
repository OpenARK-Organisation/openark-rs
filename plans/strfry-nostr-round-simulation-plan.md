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
- Codebase now uses split modules for domain objects and transport helpers.

## Current Status

### Completed
1. Implemented generic `kind:150` round messaging support in `src`.
2. Implemented Step 1 test (`tests/round_step1_nostr_kind150.rs`):
   - Steve publishes `new_round_initiate`
   - Alice receives and publishes `new_round_join`
   - strict assertions on `kind`, tags, `round_id`, and author.
3. Added verbose test logging for Step 1 (full timeline/event visibility in `--nocapture`).
4. Implemented Step 2 test (`tests/round_step2_two_actor_full_flow.rs`):
   - full two-actor flow from `new_round_initiate` to `new_round_start`
   - strict phase ordering assertions
   - verbose per-phase logs.
5. Refactored objects into separate files/modules:
   - `src/actors/provider.rs`
   - `src/actors/user.rs`
   - `src/round/message.rs`
   - `src/round/transport.rs`
   - `src/round/matchers.rs`
   - `src/wallet.rs`
   - `src/lib.rs` now module wiring + re-exports.
6. Implemented Step 3 test (`tests/round_step3_multi_actor_full_flow.rs`):
   - multi-actor flow with Alice, Bob, Carol, Dave, Eve + Steve
   - per-phase completion counters (all users required)
   - verbose actor-by-actor logging.
7. Retested successfully:
   - Step 1 test
   - Step 2 test
   - Step 3 test
   - full `cargo test` suite.

### Checkpoints Created
- `58e03e0` - Step 1 verbose two-actor kind:150 handshake.
- `66fde50` - Step 2 full two-actor kind:150 phase flow.
- `9f3f380` - module refactor + Step 3 multi-actor flow test.

### In Progress
- No active implementation in progress.

## Next Plan

### Step 4: Failure-Path Increments
1. Missing `new_round_join` from one actor (phase should not complete).
2. Invalid placeholder signature token in `new_round_vtxo_tree_accept`.
3. Invalid placeholder signature token in `new_round_start_prepared`.
4. Late `new_round_start_prepared` (after timeout).
5. Assert transition is blocked and reason is logged.

### Step 5: Repeatability/Artifacts
1. Add structured run summary output (phase result + actor statuses).
2. Add optional JSONL event trace output for replay/debug.
3. Keep tests runnable with `--nocapture` and deterministic round ids per scenario.

## Definition of Done for Current Phase
- Step 1, Step 2, and Step 3 passing in CI/local.
- Shared objects split into separate files.
- Verbose, readable logs in integration tests.
- Full test suite green after refactor and multi-actor expansion.
