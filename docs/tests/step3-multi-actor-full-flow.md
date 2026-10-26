# Step 3: Multi-Actor Full Phase Flow

**Test function:** `step3_multi_actor_kind150_full_phase_flow`
**File:** `tests/scenario/example_1.0.0/round_step3_multi_actor_full_flow.rs`
**Round ID:** `r-2026-02-24-003`

## Purpose

Validates the complete happy-path round lifecycle with one ASP and five users, exercising all seven protocol phases with concurrent participants and ASP funding pool management.

## Actors

| Actor | Role | Description |
|-------|------|-------------|
| Steve | `OpenArkServiceProvider` | Coordinates the round, manages funding pool, activates the round |
| Alice | `OpenArkUser` | Participant |
| Bob   | `OpenArkUser` | Participant |
| Carol | `OpenArkUser` | Participant |
| Dave  | `OpenArkUser` | Participant |
| Eve   | `OpenArkUser` | Participant |

## Infrastructure

- **Nostr relay:** Strfry container (`strfry-strfry:latest`) on port 7777
- **Bitcoin regtest:** Dockerized `bitcoind` via `RegtestFundingHarness` (multiple instances for wallet funding, onboard preparation, and pool seeding)
- **Tracing:** `TraceRecorder` with `OPENARK_TRACE_DIR` support

## Setup

1. Start a Strfry Nostr relay in Docker
2. Connect Steve (ASP) and all five users to the relay
3. Fund all six wallets with 2 BTC each on regtest
4. Initialize the ASP funding pool for Steve (8 UTXOs of 0.1 BTC each)
5. Prepare a confirmed 1 BTC onboard UTXO for each of the five users
6. Verify each prepared UTXO is exactly 100,000,000 sats

## Phase Sequence

### Phase 1: Round Announcement

- Steve publishes `new_round_initiate` with parameters:
  - `closing_block`: 910,000
  - `recycled_block`: 910,144
  - `leaf_timeout`: 10
- For each user (Alice, Bob, Carol, Dave, Eve):
  - User receives and validates the initiate event
  - All users see the same event ID (round reference)
  - User publishes `new_round_join` with their prepared 1 BTC onboard outpoint
  - Steve receives and validates each join
- Counter `announcement_joins` tracks all 5 joins

### Phase 2: Tree Proposal

- Steve estimates the fee budget: `300 + (200 * 5) = 1,300 sats` (worst-case vbytes at 1 sat/vB)
- Steve reserves pool UTXOs to cover the fee budget
- Steve publishes `new_round_vtxo_tree_proposal` with PSBTs built from `Step3ProposalBuildInput`:
  - Participants: `[Alice, Bob, Carol, Dave, Eve]`
  - Onboard outpoints: 2 canonical outpoints
  - Round parameters: closing=910,000, recycled=910,144, leaf_timeout=10
- Steve consumes the reserved pool UTXOs (state: Reserved -> Spent)
- For each user:
  - User receives and validates the proposal (confirms `root_psbt` matches)
  - User publishes `new_round_vtxo_tree_accept` with 2 outpoint-keyed signatures
  - Steve receives and validates each accept
- Counter `tree_accepts` tracks all 5 accepts

### Phase 3: Start Preparation

- Steve publishes `new_round_prepare_start` (references round + proposal)
- For each user:
  - User receives and validates the prepare_start event (empty content)
  - All users see the same prepare_start event ID
  - User publishes `new_round_start_prepared` with:
    - 1 funding signature
    - 2 forfeit signatures
    - 1 recycle signature
  - Steve receives and validates each start_prepared
- Counter `prepared_signals` tracks all 5 prepared signals

### Phase 4: Round Activation

- Steve publishes `new_round_start` with `round_tx: "BASE64_SIGNED_ROOT_TX_003"`
- For each user:
  - User receives and validates the round_start event
  - Parsed payload confirms the round transaction
- Counter `activated_receipts` tracks all 5 receipts

## ASP Funding Pool Lifecycle

The test exercises the full pool state machine:

| Phase | Pool Action | State Transition |
|-------|-------------|------------------|
| Setup | Initialize 8 UTXOs of 0.1 BTC | -> Available |
| Tree Proposal | `reserve_for_amount(1,300 sats)` | Available -> Reserved |
| Tree Proposal | `consume(reserved_outpoints)` | Reserved -> Spent |
| End | Summary logged | Final counts reported |

**Pool selection order:** deterministic by confirmations (ascending), then by outpoint (lexicographic).

The test records initial and final pool state:
- `initial_available_count` / `initial_available_value_sats`
- `consumed_count` / `consumed_value_sats`
- `remaining_count` / `remaining_value_sats`

### Phase 5: Unilateral Exit (All Users via CSV Timeout)

After the round completes, all five users perform independent unilateral exits on regtest — each spending their VTXO leaf output via the CSV timeout path — proving every participant can recover funds without ASP cooperation.

- A new `RegtestFundingHarness` is started with 101 blocks mined for coinbase maturity
- Steve's x-only pubkey is derived from his seed phrase
- For each user (Alice, Bob, Carol, Dave, Eve):
  - The user's account-level xprv/xpub is pre-derived at `m/86h/1h/0h`
  - A BDK miniscript descriptor is built for the VTXO leaf:
    ```
    tr(steve_xonly, {multi_a(2, user_pub/0/0, steve_xonly), and_v(v:older(10), pk(user_priv/0/0))})
    ```
  - The descriptor address is verified to match `OutputScriptFactory::create_leaf_output()`
  - The leaf output is funded with 0.5 BTC
  - An exit PSBT is built with `Sequence(10)` for CSV, signed by BDK via the timeout scriptpath
  - Pre-timeout broadcast is verified to be rejected
- After all exits are prepared, 10 blocks are mined to satisfy CSV
- All 5 exit transactions are broadcast and confirmed in a single block
- Each user's receive address is verified to hold `LEAF_FUND_SATS - EXIT_FEE_SATS` (49,980,000 sats)

| Constant | Value | Purpose |
|----------|-------|---------|
| `LEAF_FUND_SATS` | 50,000,000 | 0.5 BTC funded to each user's leaf |
| `EXIT_FEE_SATS` | 20,000 | Transaction fee per exit spend |
| `LEAF_TIMEOUT` | 10 | CSV blocks, matches round `leaf_timeout` parameter |

## Assertions

- Every event is validated as Kind 150 with correct message type tag
- Every non-initiate event carries the correct `["e", round_ref]` tag
- All users receive the same initiate, proposal, and prepare_start event IDs
- Each join payload contains exactly 1 onboard outpoint matching the prepared regtest UTXO
- Each accept payload contains exactly 2 outpoint-keyed signatures
- Each start_prepared payload parses successfully
- Phase counters all equal 5 (one per user):
  - `announcement_joins == 5`
  - `tree_accepts == 5`
  - `prepared_signals == 5`
  - `activated_receipts == 5`
- ASP pool has at least 8 available UTXOs at initialization
- `OutputScriptFactory` and BDK miniscript descriptor produce identical `script_pubkey` for each user's leaf
- CSV timeout is enforced per user: pre-timeout broadcast rejected, post-timeout succeeds
- Each user receives the correct exit amount after the timeout spend

## Trace Output

When `OPENARK_TRACE_DIR` is set, produces:
- `round_step3_multi_actor_full_flow-<timestamp>.events.jsonl` (all Nostr events with actor labels)
- `round_step3_multi_actor_full_flow-<timestamp>.summary.json` (status, duration, all phase counters)
