# ASP Funding UTXO Pool Policy (Scenario Tests)

This document defines the deterministic policy used by scenario tests when preparing ASP-owned funding UTXOs for round construction.

## Scope

- Applies to scenario tests under `tests/scenario/example_1.0.0`.
- Covers only test orchestration policy, not production wallet policy.

## Policy Constants

- `ASP_POOL_MIN_UTXO_COUNT`: minimum number of available confirmed ASP funding UTXOs before round construction begins.
- `ASP_POOL_TARGET_UTXO_VALUE_SATS`: target value for each ASP funding UTXO created for the pool.
- `ASP_POOL_MIN_CONFIRMATIONS`: minimum confirmations required before a pool UTXO is eligible for reservation.

Default test values (initial proposal):

- `ASP_POOL_MIN_UTXO_COUNT = 8`
- `ASP_POOL_TARGET_UTXO_VALUE_SATS = 10_000_000` (0.1 BTC)
- `ASP_POOL_MIN_CONFIRMATIONS = 1`

These values MAY be tuned by tests, but test behavior MUST remain deterministic.

## Eligibility Rules

A UTXO is eligible for ASP pool selection only if all of the following are true:

1. It belongs to an ASP-controlled test address.
2. It is unspent.
3. It has confirmations `>= ASP_POOL_MIN_CONFIRMATIONS`.
4. Its value is at least the requested reservation amount.

## Deterministic Coin Selection

When reserving pool UTXOs for proposal funding, tests MUST use deterministic ordering:

1. Filter to eligible UTXOs.
2. Sort by:
- ascending `confirmations`,
- then lexicographic `txid`,
- then ascending `vout`.
3. Select the first UTXO(s) needed to satisfy requested amount.
4. If multiple UTXOs are required, continue in sorted order until target is met.

Tie-break behavior MUST be stable across runs.

## Reservation and Consumption States

Pool UTXOs transition through:

- `available` -> eligible for reservation
- `reserved` -> allocated to an in-progress proposal build
- `spent` -> consumed by a broadcast transaction

A UTXO in `reserved` or `spent` MUST NOT be selected again.

## Failure Conditions

The test flow MUST fail with explicit errors if:

- available eligible UTXOs are fewer than `ASP_POOL_MIN_UTXO_COUNT` at initialization,
- reservation cannot satisfy requested amount,
- an unconfirmed UTXO is selected,
- a duplicate reservation/spend is attempted.

## Fee Sponsorship Link

This pool policy supports ASP-sponsored construction fees for the round proposal path. Fee budgeting SHOULD follow the worst-case estimate policy already documented in protocol docs.
