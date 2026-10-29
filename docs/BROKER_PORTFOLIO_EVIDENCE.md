# Account-scoped broker exposure evidence — offline validation only

Tracking: [#185](https://github.com/mlookhere/a-trade/issues/185); parent [#164](https://github.com/mlookhere/a-trade/issues/164). This is **not** an authenticated provider integration, execution permit, durable checkpoint or production authorization.

## Source authority

`01_KNOWLEDGE_ROUTER.md` routes data reconciliation to `modules/03_data_contracts.md`, portfolio/correlation risk to `modules/07_execution_risk.md`, setup ownership to `modules/02_architecture_agents.md`, and deterministic authorization to `modules/11_state_validator_schemas.md`; canonical `source/99_CANONICAL_SOURCE_VERBATIM.md` §§9, 16, 65–70, 103, 109, 122 govern the fail-closed requirements.

## Evidence contract implemented

The isolated `BrokerPortfolioEvidence::into_asserted_snapshot()` validator requires a nonblank expected account ID matching the nonblank observed account ID, a nonblank snapshot ID, explicit `TRUE` assertions for broker connection/state, positions, open-order state, data freshness, timestamps consistency and exposure completeness. Neither an empty order list nor an empty position list is proof of completeness when its state is `FALSE` or `UNKNOWN`.

Positions and open/pending orders remain separate input lists. Their externally computed positive finite risk contributions are validated and then included in one risk book. Duplicate `SETUP_ID`s within or across the lists fail closed: without provider-specific fill/order linkage the validator does **not** net a pending entry against a position, infer an accepted order has disappeared, or invent a partial-fill algorithm. The existing `SharedPortfolioReservations::from_snapshot` validator checks the combined totals and cluster directional conflicts against deployment-provided risk limits.

A successful result is an `AssertedAccountRiskSnapshot` preserving the reported account ID alongside the legacy non-authorizing `PortfolioRiskSnapshot`. Keeping the account ID prevents silent loss of declared scope during handoff. It **does not authenticate** the account or make the legacy ledger account-enforced across processes.

## Account-bound isolated reservation wrapper

The non-authorizing `AccountScopedPortfolioReservations::from_evidence` wrapper retains the validated, *asserted* account ID alongside its private in-process `SharedPortfolioReservations`. Its `reserve` boundary requires an exact, nonblank account ID **and** the underlying asserted snapshot ID, setup owner, and complete existing deterministic portfolio/cluster checks. A wrong account ID cannot mutate local risk, and shared clones continue using the same atomic in-process mutex. Read-only reservation receipts and coherent audit observations include the asserted account ID; there is no public escape hatch from the wrapper to its raw ledger.

The wrapper does **not** make account IDs cryptographically authenticated, ensure the source is the connected broker, prove timestamp freshness, or coordinate with another process/account. Existing unscoped ledger and broker gate APIs remain available, so this proof is *bypassable and strictly offline*. Account scope is a caller-asserted provenance consistency check, not a new production strategy requirement or new risk threshold.

## Explicit single-process book registry (offline proof)

`ProcessAccountRiskRegistry` is a **caller-shared** in-memory `Arc<Mutex<...>>` registry of one isolated `AccountScopedPortfolioReservations` per asserted account ID. Independently scheduled coordinators which use the **same registry instance** with identical initial account equity, configured limits, source snapshot ID and position/open-order evidence receive clones of the same risk book instead of independently spending the same in-process allowance.

After an account's first book is registered, **changed** source ID, equity, configured limits, position contents or open-order contents are a conflict, not permission to replace or reset the ledger. An account-scoped conflicting or newly UNKNOWN evidence report permanently latches the old book unavailable, retaining accepted exposure for reconciliation and audit. There is no automatic recovery, migration or release. A new exact replay of the initial evidence may reuse a healthy book, but cannot reset a latched book.

**Limitations:** the registry is *not process-global*; two independently created registries, two processes or two hosts can still double-spend. Caller-asserted evidence is unauthenticated. Validated account IDs do not prove aggregate cross-account risk. The raw standalone constructors and older Execution Gate remain bypassable. No live order may rely on this registry; #164 still owns the required durable, source-authenticated, unavoidable risk authority. This conservative evidence-conflict latch is an implementation-defined offline safety rule, not a broker fill/retry/recovery contract.

## Offline ExecutionCoordinator bridge through the asserted account registry

`ExecutionCoordinator::gate_with_process_account_registry` (Issue #191) is a **separately named mock/offline proof**, not the production Execution Gate. It runs the existing broker/setup/time preflight and then uses the caller-shared `ProcessAccountRiskRegistry` to validate the provided asserted `BrokerPortfolioEvidence`, reuse the account's single in-process book, and reserve risk with explicit owner, account ID and snapshot ID. Typed errors distinguish the existing gate rejection, registry/evidence rejection and account-book reservation rejection. A failed registry or reserve step clears only the **local untransmitted** setup reservation; accepted shared risk is never released.

The existing `gate`, `gate_with_shared_portfolio`, `submit`, raw account-book constructors, and any unrelated execution adapter remain callable independently. Two separately instantiated process registries or multiple processes do not share risk capacity. Caller-provided flags/IDs remain unverified, and the mock bridge does not prove broker stop protection, pending-order ownership, durable atomicity or live permission. The full broker-grounded non-bypassable central contract in `docs/DURABLE_PORTFOLIO_RISK_ACCEPTANCE.md` remains mandatory.

## Not implemented / no permission

The broker API and account authentication mechanism are absent. Health and time consistency flags are **assertions by the caller**, not independently verified timestamps or a trusted broker witness. Order and position risk dollars must already be determined from an approved broker/market/stop model; this validator neither prices contracts nor reconciles fill quantity or stop protection. It has no persistence, cross-process transaction, risk release, broker-side order identity, permission token, or unavoidable connection to `ExecutionCoordinator::gate`/`submit`. A matching account or snapshot ID does not make stale/unreconciled data reliable.

Production use requires the independent evidence and durability requirements in `docs/DURABLE_PORTFOLIO_RISK_ACCEPTANCE.md`, including account identity, full open/pending order and position reconciliation, non-bypassable central risk and the complete canonical §116 approval process. Until then `TRADE_ALLOWED = FALSE` for production.

## Regression evidence

Tests reject mismatched or missing account IDs, false/unknown order and broker states even with zero orders, unknown timestamps and incomplete exposure, nonpositive/nonfinite risk, duplicated setup IDs across both sources, over-cap cluster exposure and conflicting directions. Successful asserted input retains scope and counts both position and open-order exposure without netting.
