# Atomic portfolio reservation proof — in-process only

Tracking: [#165](https://github.com/mlookhere/a-trade/issues/165), parent [#164](https://github.com/mlookhere/a-trade/issues/164).

The `SharedPortfolioReservations` ledger is a deterministic **isolated engineering proof**, not an active Execution Gate or live production authorization. It protects a single shared in-process ledger with `Arc<Mutex<...>>`. Any concurrent agent thread holding a clone of the same ledger must serialize its reservation decision, avoiding a double spend of the same in-memory risk budget.

## Source authority and invariants

Canonical §§8–9, 16, 65–70, 102–103, 109–110, 122. No risk percentages or instrument-level thresholds are defaulted. An externally supplied, complete and reconciled snapshot and deployment-owned percentage ceilings initialize the book. Required state flags are `TRUE`; `FALSE` or `UNKNOWN` fail closed. These asserted flags are **not evidence** that a real broker snapshot is fresh, complete or reconciled.

Initialization also rejects pre-existing same-cluster long/short conflicts, any individual cluster already exceeding its configured risk ceiling, and non-finite computed portfolio or cluster ceilings. Equality to a configured ceiling is valid for an observed position, but no further positive risk may be reserved. These checks do not independently verify the caller-supplied broker flags.

The ledger derives risk for a sealed `OrderProposal` from its pre/post portfolio-risk delta, then **recalculates** portfolio and same-cluster budget using all exposures and reservations presently in that same book. Opposing same-cluster directions, duplicate `SETUP_ID`, wrong owner, zero/non-finite amounts, and exceeded portfolio/cluster ceilings reject without adding a reservation. A successful call does not set `execution_pass` and cannot create an `ExecutionPermit`.

Risk amounts are accumulated in lexicographic `SETUP_ID` order during both snapshot initialization and subsequent reservations. This prevents hash-map iteration order or broker snapshot vector order from changing the sequence of floating-point additions. It does not add an epsilon, round risk, change an inclusive ceiling, or prove that floating-point arithmetic is an appropriate durable broker accounting representation.

## Reservation evidence (in-process only)

A successful `reserve` call records a read-only `ReservationEvidence` lookup by `SETUP_ID`, with asserted source `snapshot_id`, cluster, direction, and reserved risk amount. Duplicates cannot replace its receipt. **Existing exposures from the initial portfolio snapshot are not newly reserved orders and return no reservation receipt.** Missing setup IDs return no evidence. This information is only in the current coordinator process and the caller's snapshot flags do not establish broker truth. Receipts cannot grant execution permission or release a reservation.

## Deliberate safety limitations

- No reservation release/reset API: release requires a future documented, broker-reconciled terminal-order/position transition. No timeout or retry is invented.
- State is **not persisted**; process restart loses reservations. It cannot authorize a live broker path.
- Multiple coordinator processes or machines do **not** share this mutex. Deployment-wide atomicity needs one durable authority and transactional updates.
- Snapshot truth and freshness must eventually be established from real broker/portfolio evidence, not caller booleans.
- This type is not wired into `ExecutionCoordinator::gate` or `submit`. Existing sealed proposal portfolio flags remain insufficient for fully autonomous concurrent live authorization.
- No strategy sequence, risk percentage, correlated cluster mapping, or production release rule is changed.

Promotion of this proof toward production must be separately implemented and verified under parent #164 with broker contracts, atomic lifecycle/ownership, persistence, failure injection and the canonical §116 approval process.
