# Account-scoped broker exposure evidence — offline validation only

Tracking: [#185](https://github.com/mlookhere/a-trade/issues/185); parent [#164](https://github.com/mlookhere/a-trade/issues/164). This is **not** an authenticated provider integration, execution permit, durable checkpoint or production authorization.

## Source authority

`01_KNOWLEDGE_ROUTER.md` routes data reconciliation to `modules/03_data_contracts.md`, portfolio/correlation risk to `modules/07_execution_risk.md`, setup ownership to `modules/02_architecture_agents.md`, and deterministic authorization to `modules/11_state_validator_schemas.md`; canonical `source/99_CANONICAL_SOURCE_VERBATIM.md` §§9, 16, 65–70, 103, 109, 122 govern the fail-closed requirements.

## Evidence contract implemented

The isolated `BrokerPortfolioEvidence::into_asserted_snapshot()` validator requires a nonblank expected account ID matching the nonblank observed account ID, a nonblank snapshot ID, explicit `TRUE` assertions for broker connection/state, positions, open-order state, data freshness, timestamps consistency and exposure completeness. Neither an empty order list nor an empty position list is proof of completeness when its state is `FALSE` or `UNKNOWN`.

Positions and open/pending orders remain separate input lists. Their externally computed positive finite risk contributions are validated and then included in one risk book. Duplicate `SETUP_ID`s within or across the lists fail closed: without provider-specific fill/order linkage the validator does **not** net a pending entry against a position, infer an accepted order has disappeared, or invent a partial-fill algorithm. The existing `SharedPortfolioReservations::from_snapshot` validator checks the combined totals and cluster directional conflicts against deployment-provided risk limits.

A successful result is an `AssertedAccountRiskSnapshot` preserving the reported account ID alongside the legacy non-authorizing `PortfolioRiskSnapshot`. Keeping the account ID prevents silent loss of declared scope during handoff. It **does not authenticate** the account or make the legacy ledger account-enforced across processes.

## Not implemented / no permission

The broker API and account authentication mechanism are absent. Health and time consistency flags are **assertions by the caller**, not independently verified timestamps or a trusted broker witness. Order and position risk dollars must already be determined from an approved broker/market/stop model; this validator neither prices contracts nor reconciles fill quantity or stop protection. It has no persistence, cross-process transaction, risk release, broker-side order identity, permission token, or unavoidable connection to `ExecutionCoordinator::gate`/`submit`. A matching account or snapshot ID does not make stale/unreconciled data reliable.

Production use requires the independent evidence and durability requirements in `docs/DURABLE_PORTFOLIO_RISK_ACCEPTANCE.md`, including account identity, full open/pending order and position reconciliation, non-bypassable central risk and the complete canonical §116 approval process. Until then `TRADE_ALLOWED = FALSE` for production.

## Regression evidence

Tests reject mismatched or missing account IDs, false/unknown order and broker states even with zero orders, unknown timestamps and incomplete exposure, nonpositive/nonfinite risk, duplicated setup IDs across both sources, over-cap cluster exposure and conflicting directions. Successful asserted input retains scope and counts both position and open-order exposure without netting.
