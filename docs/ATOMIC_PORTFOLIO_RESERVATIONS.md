# Atomic portfolio reservation proof — in-process only

Tracking: [#165](https://github.com/mlookhere/a-trade/issues/165), parent [#164](https://github.com/mlookhere/a-trade/issues/164).

The `SharedPortfolioReservations` ledger is a deterministic **isolated engineering proof**, not an active Execution Gate or live production authorization. It protects a single shared in-process ledger with `Arc<Mutex<...>>`. Any concurrent agent thread holding a clone of the same ledger must serialize its reservation decision, avoiding a double spend of the same in-memory risk budget.

## Source authority and invariants

Canonical §§8–9, 16, 65–70, 102–103, 109–110, 122. No risk percentages or instrument-level thresholds are defaulted. An externally supplied, complete and reconciled snapshot and deployment-owned percentage ceilings initialize the book. Required state flags are `TRUE`; `FALSE` or `UNKNOWN` fail closed. These asserted flags are **not evidence** that a real broker snapshot is fresh, complete or reconciled.

Initialization also rejects pre-existing same-cluster long/short conflicts, any individual cluster already exceeding its configured risk ceiling, and non-finite computed portfolio or cluster ceilings. Equality to a configured ceiling is valid for an observed position, but no further positive risk may be reserved. These checks do not independently verify the caller-supplied broker flags.

The ledger derives risk for a sealed `OrderProposal` from its pre/post portfolio-risk delta, then **recalculates** portfolio and same-cluster budget using all exposures and reservations presently in that same book. Opposing same-cluster directions, duplicate `SETUP_ID`, wrong owner, zero/non-finite amounts, and exceeded portfolio/cluster ceilings reject without adding a reservation. A successful call does not set `execution_pass` and cannot create an `ExecutionPermit`.

Risk amounts are accumulated in lexicographic `SETUP_ID` order during both snapshot initialization and subsequent reservations. This prevents hash-map iteration order or broker snapshot vector order from changing the sequence of floating-point additions. It does not add an epsilon, round risk, change an inclusive ceiling, or prove that floating-point arithmetic is an appropriate durable broker accounting representation.

## Caller snapshot identity at reservation time

Each `reserve(proposal, owner, expected_snapshot_id)` call must name the nonblank, **exact** source snapshot ID supplied when this ledger was constructed. The comparison and risk mutation happen under one mutex; wrong or missing identities return `SnapshotMismatch` without recording a reservation or receipt. The prior in-process invalidation latch still takes precedence and remains irreversible, including for a caller with a matching identity. A new ledger can only be initialized from a separately validated new snapshot; there is no reset method.

This is an **implementation-defined provenance safeguard**, not a canonical requirement to authenticate a broker through an ID string. Caller-provided matching IDs do not prove source timeliness, account ownership, equity authenticity or process-wide synchronization. This does not integrate the ledger with any broker or live Execution Gate.

## Reservation evidence (in-process only)

A successful `reserve` call records a read-only `ReservationEvidence` lookup by `SETUP_ID`, with asserted source `snapshot_id`, cluster, direction, and reserved risk amount. Duplicates cannot replace its receipt. **Existing exposures from the initial portfolio snapshot are not newly reserved orders and return no reservation receipt.** Missing setup IDs return no evidence. This information is only in the current coordinator process and the caller's snapshot flags do not establish broker truth. Receipts cannot grant execution permission or release a reservation.

## Fail-closed invalidation after snapshot health failure

Canonical §§16, 98–100 require invalid broker, market-data, open-order, or position state to stop new trades. The in-process ledger supports an implementation-defined conservative `invalidate()` latch: after an **external** Data Supervisor detects invalidity, calling it permanently rejects subsequent `reserve` calls from all clones sharing that ledger. The latch and reservation mutation use the **same mutex**, so concurrent invalidation cannot race past a later reservation. It does not erase prior exposure, accepted reservation evidence, or broker-reconciliation obligations. Multiple calls are idempotent. There is no reset/re-enable method: recovery requires a separately validated new snapshot and ledger instance.

This API does **not** independently monitor timestamps, verify broker state, authenticate its caller, persist the latch, synchronize across processes, or protect live order execution. Neither a caller's claimed healthy snapshot nor this in-memory latch is evidence of production readiness. The current live Execution Gate remains unmodified under parent #164.

## Coherent read-only audit observation

The `audit_view()` method copies the current ledger's asserted source snapshot ID, account equity, configured risk ceilings, full SETUP_ID-sorted exposures (both original broker baseline and new reservations), SETUP_ID-sorted accepted reservation identifiers, and permanent invalidation state under **one mutex acquisition**. A racing reservation or invalidation is observed entirely before or entirely after its mutation, avoiding an internally torn view. No read operation reserves risk, resets a latch, releases a reservation, or issues an execution permit.

This value is **not a durable checkpoint, serialized state schema, authenticated broker snapshot, or authority to resume trading after process restart**. An external log of the view cannot safely restore or release reservations without a separately specified broker-reconciled persistence lifecycle. The initial baseline exposures remain distinguishable from SETUP_IDs accepted by this ledger.

## Offline execution-coordinator integration proof

The optional `ExecutionCoordinator::gate_with_shared_portfolio` method (Issue #181) first applies the existing setup/time/broker/operational preflight and then asks the shared in-process ledger to atomically reserve risk using the submitting owner and asserted snapshot ID. It returns a typed `PortfolioCoordinatedGateError` distinguishing an existing gate rejection from a reservation failure. If the latter fails, only the **untransmitted local** setup reservation is cleared; the rejected shared ledger is unchanged. An accepted shared reservation is retained conservatively if a later broker step fails; no speculative release is added. Synthetic tests demonstrate two independent coordinators competing for one configured risk allowance.

**Important: this is an offline integration proof only.** The existing `gate` and `submit` APIs can still be used without this helper. There is no authenticated live broker snapshot, durable or distributed shared ledger, restart reconciliation, broker-terminal risk-release contract, or non-bypassable central authorization requirement. In particular, an application must never treat the presence of this helper or its green CI as live execution readiness. Parent #164 continues to own the mandatory non-bypassable, broker-reconciled risk authority.

## Deliberate safety limitations

- No reservation release/reset API: release requires a future documented, broker-reconciled terminal-order/position transition. No timeout or retry is invented.
- State is **not persisted**; process restart loses reservations. It cannot authorize a live broker path.
- Multiple coordinator processes or machines do **not** share this mutex. Deployment-wide atomicity needs one durable authority and transactional updates.
- Snapshot truth and freshness must eventually be established from real broker/portfolio evidence, not caller booleans.
- This type is not wired into `ExecutionCoordinator::gate` or `submit`. Existing sealed proposal portfolio flags remain insufficient for fully autonomous concurrent live authorization.
- No strategy sequence, risk percentage, correlated cluster mapping, or production release rule is changed.

Promotion of this proof toward production must be separately implemented and verified under parent #164 with broker contracts, atomic lifecycle/ownership, persistence, failure injection and the canonical §116 approval process.
