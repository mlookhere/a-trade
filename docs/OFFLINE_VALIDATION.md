# Offline Replay and Out-of-Sample Validation

Canonical §116 requires proposed strategy modifications to progress through:

`hypothesis -> backtest -> out-of-sample test -> paper/shadow execution -> approval -> new version`

This repository's offline harness implements only the **backtest** and **out-of-sample** evidence stages. It does not promote a strategy, approve a change, create a new strategy version, or simulate paper/shadow execution.

`ValidationDatasetManifest` freezes explicit backtest and out-of-sample session IDs for a named strategy version. The two sets must be nonempty, internally unique, and disjoint. This is validation infrastructure, not a production trading rule.

`ReplayCaseEvidence` records expected and observed deterministic outputs from the existing Rust core. The harness does not contain a second copy of strategy logic. Evidence is keyed by case ID, session ID, and SETUP_ID; duplicate case evidence or duplicate `(session, SETUP_ID)` evidence is rejected.

`ReplayReport` reports exact matches and authorization/rejection/state mismatches. Canonical knowledge does not specify a minimum win rate, expectancy, profit factor, drawdown, or replay-match percentage for promotion, so this module intentionally defines no automatic pass threshold.

Historical market-data file formats, fill/slippage simulation, provider adapters, paper/shadow execution, human approval, and version issuance are separate concerns and remain outside this module until their contracts are specified.
