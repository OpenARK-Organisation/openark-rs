# Step 2: Two-Actor Full Phase Flow

**Test function:** `step2_two_actor_kind150_full_phase_flow`
**File:** `tests/scenario/example_1.0.0/round_step2_two_actor_full_flow.rs`
**Round ID:** `r-2026-02-24-002`

## Purpose

Validates the complete happy-path round lifecycle between a single ASP (Steve) and a single user (Alice), exercising all seven protocol phases over Nostr Kind 150 events.

## Actors

| Actor | Role | Description |
|-------|------|-------------|
| Steve | `OpenArkServiceProvider` | Coordinates the round, publishes proposals, activates the round |
| Alice | `OpenArkUser` | Joins the round, signs proposals, submits prepared signatures |

## Infrastructure

- **Nostr relay:** Strfry container (`strfry-strfry:latest`) on port 7777
- **Bitcoin regtest:** Dockerized `bitcoind` via `RegtestFundingHarness`
- **Tracing:** `TraceRecorder` with `OPENARK_TRACE_DIR` support

## Setup

1. Start a Strfry Nostr relay in Docker
2. Connect Steve (ASP) and Alice (user) to the relay
3. Fund both wallets with 2 BTC each on regtest (101-block coinbase maturation + 1 confirmation)
4. Initialize the ASP funding pool for Steve (8 UTXOs of 0.1 BTC each, 1 confirmation minimum)
5. Prepare a confirmed 1 BTC onboard UTXO for Alice

## Phase Sequence

### Phase 1: Round Announcement

- Steve publishes `new_round_initiate` with parameters:
  - `closing_block`: 910,000
  - `recycled_block`: 910,144
  - `leaf_timeout`: 10
- Alice receives and validates the initiate event
- The initiate event ID becomes the **round reference** for all subsequent messages
- Alice publishes `new_round_join` with:
  - 1 onboard outpoint (her prepared 1 BTC UTXO)
  - 0 offboard entries
  - `user_pubkey`: her BDK-derived round key
  - `agent_pubkey`: `"Alice-agent-pubkey"` (placeholder)
- Steve receives and validates the join event, confirming event reference matches the round

### Phase 2: Tree Proposal

- Steve publishes `new_round_vtxo_tree_proposal` containing PSBTs:
  - `root_psbt`, `vtxo_tree_psbts[]`, `forfeit_tree_psbts[]`, `recycle_psbt`
- Alice receives and validates the proposal
- Alice publishes `new_round_vtxo_tree_accept` with outpoint-keyed signatures:
  - `aaaa...:0` -> `BASE64_SIG_V0_Alice`
  - `bbbb...:1` -> `BASE64_SIG_V1_Alice`
- Steve receives and validates the accept event (checks both round ref and proposal ref)

### Phase 3: Start Preparation

- Steve publishes `new_round_prepare_start` (empty content, references round + proposal)
- Alice receives and validates the prepare_start event
- Alice publishes `new_round_start_prepared` with:
  - `funding_signatures`: `{ffff...:0 -> BASE64_FUNDING_SIG_Alice}`
  - `forfeit_signatures`: `{dddd...:0 -> BASE64_FORFEIT_SIG_0_Alice, eeee...:1 -> BASE64_FORFEIT_SIG_1_Alice}`
  - `recycle_signature`: `BASE64_RECYCLE_SIG_Alice`
- Steve receives and validates the start_prepared event

### Phase 4: Round Activation

- Steve publishes `new_round_start` with `round_tx: "BASE64_SIGNED_ROOT_TX_002"`
- Alice receives and validates the round_start event
- Parsed payload confirms the round transaction matches

### Phase 5: Unilateral Exit (Alice's CSV Timeout Spend)

After the round completes, Alice performs a unilateral exit on regtest — spending her VTXO leaf output via the CSV timeout path — proving she can recover her funds without ASP cooperation.

- A new `RegtestFundingHarness` is started with 101 blocks mined for coinbase maturity
- Keys are derived from the actors' seed phrases:
  - Alice's x-only pubkey (from `round_user_pubkey_hex()`)
  - Steve's x-only pubkey (from `derive_bip340_xonly_pubkey_from_seed_phrase`)
  - Alice's master xprv (from `derive_master_xprv_base58_from_seed_phrase`)
- A BDK miniscript descriptor is built for the VTXO leaf:
  ```
  tr(steve_xonly, {multi_a(2, alice_xonly, steve_xonly), and_v(v:older(10), pk(alice_xprv/86h/1h/0h/0/0))})
  ```
- The descriptor address is verified to match `OutputScriptFactory::create_leaf_output()` — confirming that the protocol's script factory and the miniscript descriptor produce identical Taproot outputs
- The leaf output is funded with 0.5 BTC (simulating the root tx creating the VTXO tree)
- An exit PSBT is built spending the leaf to Alice's wallet receive address with `Sequence(10)` for CSV
- BDK signs via the timeout scriptpath (the only path where Alice has a private key)
- **CSV enforcement is verified:**
  - Pre-timeout broadcast is rejected as non-final
  - After mining 10 blocks, broadcast succeeds
- Alice's receive address is confirmed to hold `LEAF_FUND_SATS - EXIT_FEE_SATS` (49,980,000 sats)

| Constant | Value | Purpose |
|----------|-------|---------|
| `LEAF_FUND_SATS` | 50,000,000 | 0.5 BTC funded to the leaf |
| `EXIT_FEE_SATS` | 20,000 | Transaction fee for the exit spend |
| `LEAF_TIMEOUT` | 10 | CSV blocks, matches round `leaf_timeout` parameter |

## Assertions

- Every event is validated as Kind 150 with correct message type tag
- Every non-initiate event carries the correct `["e", round_ref]` tag
- Message ordering is verified at the end:
  ```
  INITIATE -> JOIN -> VTXO_TREE_PROPOSAL -> VTXO_TREE_ACCEPT ->
  PREPARE_START -> START_PREPARED -> START
  ```
- All payloads round-trip correctly through serialize/parse
- Alice's onboard outpoint in the join matches her prepared regtest UTXO
- `OutputScriptFactory` and BDK miniscript descriptor produce identical `script_pubkey` for the leaf
- CSV timeout is enforced: pre-timeout broadcast is rejected, post-timeout succeeds
- Alice receives the correct exit amount after the timeout spend

## Trace Output

When `OPENARK_TRACE_DIR` is set, produces:
- `round_step2_two_actor_full_flow-<timestamp>.events.jsonl` (all Nostr events with actor, label, tags, content)
- `round_step2_two_actor_full_flow-<timestamp>.summary.json` (status, duration, observed vs expected order)
