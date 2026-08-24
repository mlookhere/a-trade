# Strategy Change Promotion Boundary

Canonical §116 requires every proposed strategy modification to follow this order:

`hypothesis -> backtest -> out-of-sample test -> paper/shadow execution -> approval -> new version`

`StrategyChangePromotion` enforces that ordering as non-live evidence bookkeeping. A stage cannot be skipped, repeated, or reordered.

Backtest and out-of-sample stages consume `ReplayReport` evidence from the offline validation layer. A report must be structurally consistent, but its results are **not** converted into an automatic pass/fail threshold because canonical knowledge defines no minimum replay match, win rate, expectancy, profit factor, drawdown, or other promotion threshold.

Paper/shadow execution is represented only by an opaque external evidence identifier plus `Paper` or `Shadow` mode. This module does not simulate fills, slippage, market data, broker behavior, or paper accounts because those semantics are not specified by canonical knowledge.

Approval is an externally supplied tri-state decision. `UNKNOWN` cannot advance. `FALSE` rejects the proposal. `TRUE` permits the final bookkeeping step only.

The final `new version` identifier is externally supplied, must be nonempty, and must differ from the base strategy version. Recording a new version does **not** activate it, deploy it, alter live parameters, or change the current production strategy. No live activation API exists in this module.
