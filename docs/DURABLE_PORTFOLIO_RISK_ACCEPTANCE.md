# Durable portfolio-risk authority — acceptance contract (NOT implemented)

Tracking: [#183](https://github.com/mlookhere/a-trade/issues/183), parent [#164](https://github.com/mlookhere/a-trade/issues/164). This is a **development acceptance specification**, not a broker adapter, a new canonical strategy rule, or authority to place orders.

## Governing authority

Read `01_KNOWLEDGE_ROUTER.md` and the exact canonical sources: `modules/02_architecture_agents.md`, `modules/03_data_contracts.md`, `modules/07_execution_risk.md`, `modules/08_position_management.md`, `modules/11_state_validator_schemas.md`, `modules/12_audit_metrics.md`; `source/99_CANONICAL_SOURCE_VERBATIM.md` §§9, 16, 64–70, 98–103, 109–112, 118, 122. Higher-authority canonical text prevails over this document.

**Canonical invariants:** every new entry requires a complete ordered setup and deterministic risk/portfolio/correlation/duplicate/broker/engine approval; `UNKNOWN = NO_TRADE` (§§16, 102, 109, 122). Existing and proposed exposure must fit deployment-configured aggregate and correlated-cluster ceilings (§§65–70); parallel agents cannot double-spend allowance or duplicate a `SETUP_ID` (§§9, 103). A filled entry without a confirmed hard stop requires immediate flatten (§64). Broker/order/position uncertainty blocks new entries while already-filled positions still require protective management (§§16, 64, 118).

## What exists today, and what is NOT proven

- `SharedPortfolioReservations`: in-process `Arc<Mutex<...>>`, explicit snapshot ID and caller-asserted health flags, deterministic portfolio/cluster re-evaluation, immutable accepted exposure, read-only receipts, permanent invalidation, and coherent audit observations. **No cross-process durability, data-source authentication, transactional broker lifecycle, or release.**
- `ExecutionCoordinator::gate_with_shared_portfolio` (merged #181/#182) is **optional/offline** and therefore bypassable via the existing `gate` and `submit` paths. Its success is not a valid production-risk proof.
- `BrokerAdapter::reconcile` supplies coarse tri-state broker/position/order facts and setup IDs. It does **not** yet supply authenticated account equity, priced open-position risk and protection state, cross-provider correlated exposure, or a transactionally consistent order/position snapshot. A truthful `Condition::True` cannot be inferred from its presence.
- The in-process audit view is **not** a replayable durable checkpoint. Reconstructing entries, releases or broker orders from it would invent unsupported lifecycle semantics.
- The real broker execution, OCO/bracket, partial fill, cancel/replace, order acknowledgement, polling/retry and restart contracts remain provider-specific and **unspecified**.

## Required evidence inputs before any live authorization

| Input / proof | Source authority | Fail-closed requirement |
| --- | --- | --- |
| Account and broker identity, configured scope and equity | §§16, 65–69, 109 | Unknown, mismatched or unreconciled account/equity cannot authorize new entries. Configuration contains **no inferred risk percentage**. |
| Broker positions **and open/pending orders** for the relevant account(s) | §§9, 16, 67–70, 109 | Complete current exposure across all participating agents/instruments must be reconciled; missing/contradictory states reject. Pending accepted orders cannot disappear from reserved capacity. |
| Market timestamps, data availability and synchronization | §§11–16, 109 | Data Supervisor validates the actual feed/snapshot timestamps. No invented numeric freshness threshold or statement that a matching string is fresh. |
| Canonical setup identity, owner, direction and state | §§9, 102–103 | The exact `SETUP_ID` cannot be claimed, transmitted or re-entered by two independent owners; no sequence skipping. |
| Correlation cluster identity and risk contribution | §§67–70 | Compare *current open and reserved risk plus proposed risk* with the explicitly configured portfolio and cluster ceilings. Conflicting same-cluster direction requires conflict review, not independent admission. |
| Broker bracket and stop evidence | §§16, 64, 118 | An accepted/filling entry cannot be treated as protected without broker-confirmed hard-stop evidence. Unprotected confirmed fill triggers the specified immediate flatten. |
| Operational violations and audit witness | §§98–103, 109, 111–112 | Shared broker/risk failures stop new entry; capture SETUP_ID, source version, pre-state, decision, broker action (or absence), and reason. |

## Transactional authority to implement (engineering requirement, not broker behavior)

**One authoritative, durable, cross-process reservation service** must provide a single transactional decision for all agent workers controlling the same configured portfolio and correlated clusters. The implementation must prove that two processes observing the same available allowance cannot both commit excess risk. An in-process mutex, caller-provided snapshot ID, two separate database reads/writes, or a green single-process test is insufficient.

A prospective transaction must prove, as one indivisible logical decision:

1. The requested `SETUP_ID` and owner are unique and match the sealed canonical proposal (§§9, 102–103, 110).
2. A trusted Data Supervisor/broker reconciliation witness is present, current, account-consistent and complete (§16); any required field that is unavailable is **UNKNOWN**, not TRUE.
3. Every open and accepted-but-not-terminal broker position/order and every outstanding reservation is counted once; calculated risk amounts are finite and bounded (§§65–69).
4. All portfolio and correlation checks pass using the **same committed version** of the shared exposure state (§§67–70, 109).
5. The reservation is durably committed before it may be relied upon by an execution permit. A repeated `SETUP_ID` cannot produce a second grant; ambiguous commit outcome does **not** justify a blind retry, release, or duplicate broker order.

**Not specified / requires explicit versioned implementation design:** persistence technology; locking or compare-and-swap protocol; account identity binding and broker source-of-truth API; clock/freshness contract; exact risk reconciliation on partial fills; order acknowledgement and timeout semantics; cross-account aggregation; transaction recovery and broker-terminal release proof. None may be inferred from the code or adopted from a brokerage convention. Treat unresolved values and conditions as non-authorizing.

## Entry, order and risk lifecycle acceptance matrix

| Fault or transition | Must be demonstrated | Boundary |
| --- | --- | --- |
| Two processes reserve for correlated instruments at one remaining allowance | At most the capacity-valid nonduplicate set commits; no double-spend and no independent acceptance from stale copies | §§9, 67–70, 103 |
| Snapshot/account ID matches but broker positions/orders disagree | New entry rejected; not converted to healthy simply because source IDs match | §§16, 109 |
| Duplicate SETUP_ID across different agents or process restart | No second accepted order; original owner and broker exposure stay traceable | §§9, 102–103 |
| Process dies between risk validation and durable reserve commit | No authorization on an unproven commit; restart reconciles before new entries | §§16, 67, 109 |
| Process dies after reserve but before transmission | Capacity remains conservatively accounted for until a broker-grounded terminal/reconciliation contract proves release | §§16, 67–69, 109; implementation-defined conservative treatment |
| Submit or acknowledgement is ambiguous, disconnected or late | No retry that could duplicate an order; mark unknown and block new entry until broker truth is established | §§9, 16, 64, 100, 109 |
| Partial fill, quantity discrepancy, or stop/target mismatch | No guessed release or risk reduction; require documented real broker fill and protection semantics | §§16, 64, 67, 118 |
| Confirmed fill but hard stop unconfirmed | Attempt immediate flatten and flag execution failure, independently of new-entry lockout | §§64, 100, 118 |
| Pending-order cancellation, expiry or reject | Never release based on a local intention; require broker-verified terminal and remaining exposure; **exact provider race behavior unknown** | §§9, 16, 60–61, 67 |
| External kill switch, market data invalidity or portfolio discrepancy | New entry blocked across all affected workers and processes; previously open positions remain subject to protection checks | §§16, 98–101, 118 |
| Broker returns authoritative changed portfolio exposure during execution | Transaction must re-evaluate/resynchronize; no stale permit may overrule current portfolio/correlation limits | §§16, 67–70, 109 |
| Invalid timestamp, conflicting order, malformed data, failed persistence | Deterministic reject with audit evidence; never send new broker order | §§16, 100, 109, 112 |

Each eventual implementation test must include **input witness → pre-state → decision/rejection → durable state → broker-visible action or no action**, plus crash/restart replay evidence. Test both equality and just-over-cap boundaries using externally supplied limits; do not invent default deployment values.

## Evidence traceability for follow-up implementation reviews

For each proposed durable-risk implementation PR, reviewers must be able to follow one `SETUP_ID` through **all** of the following boundaries, including negative and UNKNOWN cases. These are acceptance witnesses, **not** a new canonical wire schema or an assertion that the present code supplies them.

| Boundary | Evidence to present | Current status |
| --- | --- | --- |
| Data Supervisor → Portfolio Coordinator | Account-scoped, reconciled positions, pending/open orders, configured limits, instrument/cluster mappings and validated time/source identity | Caller-asserted health flags and a snapshot ID only; provider truth is **unverified** |
| Portfolio Coordinator → durable risk authority | One atomic commit decision binding SETUP_ID, owner, source witness, complete current/reserved exposure and the resulting portfolio/cluster verdict | `SharedPortfolioReservations` supplies only a process-local mutex; crash and multi-process guarantees are **absent** |
| Durable risk authority → Execution Gate | Proof that a committed, still-valid reservation is mandatory and cannot be bypassed by another order submission path | `gate_with_shared_portfolio` is optional; existing `gate`/`submit` interfaces do **not** enforce this |
| Execution Gate → broker | Broker-visible order identity, accepted/rejected/unknown acknowledgement and corresponding setup/position state evidence | Generic adapter/fake tests exist; provider-specific identity and ambiguous acceptance are **unspecified** |
| Broker event → protection/reconciliation | Confirmed fill quantity, hard-stop acknowledgement, outstanding order/position exposure and a documented safe reservation lifecycle | Canonical stop/flatten duty exists (§§64, 118); provider terminal and partial-fill release conditions remain **unresolved** |
| All paths → Audit | Original input witness, deterministic before/after state, rejection reason, reservation result and broker action or documented absence | In-memory receipts and audit observations are not a durable transactional event log |

A reviewer must **reject claims of live readiness** when any row is unsupported, even if the submitted tests pass or a snapshot ID matches. Any eventual recovery design must explicitly distinguish *observed broker position/order truth* from *local intended order state*; absence of a local pending record is not proof the broker lacks an order (§§9, 16, 103, 109). A future implementation may choose a persistence mechanism only through a separately reviewed design with demonstrated atomicity, restart recovery and broker reconciliation; this document does not choose one.

## Required architecture promotion gate

Before replacing offline proof with live execution authority, evidence must establish all of the following **simultaneously**:

- The public entry/submit path is **non-bypassable**: every new order requires the real deterministic Portfolio Coordinator's durable committed reservation and the full Strategy Validator/Risk Engine/Execution Gate sequence (§§102, 109, 122).
- The same reservation and broker/order identity survive process crash, disconnect, retry and agent restart without duplicate exposure or falsely released capacity (§§9, 16, 67–70, 103).
- A broker- and deployment-specific adapter has independently verified account/positions/open orders, bracket/stop confirmation, ambiguous acknowledgements, partial fill and cancellation behavior. Unknown provider responses do not become TRUE (§§16, 64, 118).
- Externally configured risk limits, correlation groups and emergency limits are approved and tested; examples are not promoted to universal defaults (§§65–70, 98–99).
- Branch enforcement [#26](https://github.com/mlookhere/a-trade/issues/26) and the complete §116 historical/OOS/paper-shadow/human approval workflow are independently verified.

**No schema in this document is an approved serialized wire contract. No transition or provider behavior omitted by canonical source is silently implemented.** Until all evidence exists, `TRADE_ALLOWED = FALSE` for production.
