# Offline Replay and Out-of-Sample Validation

Canonical §116 requires proposed strategy modifications to progress through:

`hypothesis -> backtest -> out-of-sample test -> paper/shadow execution -> approval -> new version`

This repository's offline harness implements only the **backtest** and **out-of-sample** evidence stages. It does not promote a strategy, approve a change, create a new strategy version, or simulate paper/shadow execution.

`ValidationDatasetManifest` freezes explicit backtest and out-of-sample session IDs for a named strategy version. The two sets must be nonempty, internally unique, and disjoint. This is validation infrastructure, not a production trading rule.

`ReplayCaseEvidence` records expected and observed deterministic outputs from the existing Rust core. The harness does not contain a second copy of strategy logic. Evidence is keyed by case ID, session ID, and SETUP_ID; duplicate case evidence or duplicate `(session, SETUP_ID)` evidence is rejected.

`ReplayReport` reports exact matches and authorization/rejection/state mismatches. Canonical knowledge does not specify a minimum win rate, expectancy, profit factor, drawdown, or replay-match percentage for promotion, so this module intentionally defines no automatic pass threshold.

Historical market-data file formats, fill/slippage simulation, provider adapters, paper/shadow execution, human approval, and version issuance are separate concerns and remain outside this module until their contracts are specified.


## AI-native research-only paired observation layer

`gex-strategy` has a separate, **non-authorizing** `ResearchPairedEvaluationManifest` for explicitly versioned, disjoint backtest and out-of-sample session partitions. It does not modify `auction-core`'s canonical replay harness or the §116 promotion coordinator.

`ResearchPairedReplayCase::record` binds an externally supplied, unverified `StrictSourceBaseline`-mode direction annotation to an existing `AiNative` `ResearchDecisionProposal` from the exact same immutable `MarketSnapshot`. The baseline record identifier is caller-supplied provenance, **not** proof that source-baseline strategy rules were executed. The AI-native proposal itself is a hypothesis with original four-witness evidence and adversarial review, **not** a validated entry decision.

`summarize_research_pairs` counts only same known advisory direction, different known advisory direction, and cases with an UNKNOWN advisory direction; it rejects duplicate case IDs, duplicate session/snapshot pairs, mixed phases, and revised study manifests. These directional-concordance counts are **not** expectancy, profit factor, drawdown, win rate, an empirical advantage, a calibrated probability, an execution authorization, or a strategy-promotion approval.

A real paired evaluation still requires captured historical inputs, an implemented source-baseline evaluator, source-truth validation, outcome labeling, replay simulation, out-of-sample analysis, paper/shadow execution, and explicit approval. There are no default numerical acceptance thresholds or inferred broker fill assumptions.
