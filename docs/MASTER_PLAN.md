# Master plan — Multi-Agent First-90-Minute Auction Trading System

**Document role:** durable development roadmap. **Live checkpoint:** [Issue #1 — CONTROL](https://github.com/mlookhere/a-trade/issues/1). **Last reconciled:** 2026-10-28. Read the control issue, open Issues/PRs, branch heads, and exact-head Actions checks before taking action; this document alone is never live state.

## Objective and source authority

Build and validate a deterministic, fail-closed multi-agent trading system without giving LLMs independent trading authority. The 123-section First-90-Minute Auction knowledge set (fingerprint `9e24b9a4ffc55a8815cb62e5c9f72d8356a3d52de5e716a2db8ced38df508deb`) is authoritative for the **canonical auction path**. Locate governing rules through `01_KNOWLEDGE_ROUTER.md`, then read the exact modules and, when necessary, the canonical numbered source sections from the Project Source ZIPs. Existing code, tests, this roadmap, and research do not override the canonical source.

The canonical order is **environment → location → participation → effort versus result → absorption → dominance shift → second attempt → second failure → reconfirmation → risk validation → execution**. `TRADE_ALLOWED = FALSE` unless all ordered gates pass; `UNKNOWN = NO_TRADE`. The Strategy Validator, Risk Engine, Portfolio Coordinator, and Execution Gate retain deterministic final authority. A single indicator (including Fib, delta, absorption, or GEX) never authorizes entry. Entries are restricted to 09:30:00–10:59:59 ET, while existing-position management remains allowed after cutoff. Setup ownership/consumption, deduplication, portfolio/correlation limits, broker reconciliation, stop safety, and audit trails are mandatory.

## Three separate development lanes

| Lane | Repository surface | Authority | Current boundary |
| --- | --- | --- | --- |
| Canonical auction implementation | `crates/auction-core` | The 123 numbered auction sections | Deterministic production **candidate**, not approved for live trading |
| Provider/data infrastructure | `crates/gex-engine`, `crates/schwab-gex-adapter` | Source-valid evidence transport and arithmetic only | No strategy, order, risk, or direction authorization |
| AI-native Ninja GEX vNext | `crates/gex-strategy` | Approved research target, with unapproved formalizations labeled | Research/validation only, isolated from live broker authorization |

Production changes cannot be inferred from a successful research feature, AI result, backtest, or green CI. Material strategy promotion requires **hypothesis → backtest → out-of-sample → paper/shadow execution → human approval → new version**, as prescribed by canonical §116.

## Milestones and dependencies

| Phase | Deliverable | Tracking | State |
| --- | --- | --- | --- |
| 1. Control plane and canonical safety | CI, setup state, deterministic validator, central risk/execution, data validity, orderflow, position management, audit/replay contracts | Completed Issues/PRs summarized in [#1](https://github.com/mlookhere/a-trade/issues/1); implementation and evidence in `auction-core` | Implemented development slices; no live release |
| 2. Provider and GEX transport | Provider-neutral GEX engine, dependency security, Schwab multi-profile stream, snapshot/coverage/provenance protections | [#53](https://github.com/mlookhere/a-trade/issues/53), [#55](https://github.com/mlookhere/a-trade/issues/55), [#56](https://github.com/mlookhere/a-trade/issues/56), [#73](https://github.com/mlookhere/a-trade/issues/73), [#76](https://github.com/mlookhere/a-trade/issues/76), [#78](https://github.com/mlookhere/a-trade/issues/78), [#80](https://github.com/mlookhere/a-trade/issues/80) | Initial infrastructure slices merged; full provider validity not yet established |
| 3. Research deterministic features | Explicit snapshot-bound observations, 13-condition hard-veto transport, specialist evidence interfaces, OHLCV coverage and carefully versioned numerical features | [#57](https://github.com/mlookhere/a-trade/issues/57), [#68](https://github.com/mlookhere/a-trade/issues/68), [#70](https://github.com/mlookhere/a-trade/issues/70), [#72](https://github.com/mlookhere/a-trade/issues/72), [#82](https://github.com/mlookhere/a-trade/issues/82) / [PR #83](https://github.com/mlookhere/a-trade/pull/83) | Ongoing; EMA remains research-only pending exact-head CI and merge |
| 4. AI-native decision architecture | Stable shared feature set; specialist observations; playbook eligibility, interactions, adversarial review, hierarchical arbiter, calibrated-EV interface, deterministic vNext execution validation, outcome persistence | Create independently scoped Issues after prerequisite data/feature contracts are verified | Future work; not production authorization |
| 5. Empirical validation and release evidence | Historical data/replay, source baseline vs AI-native comparison, OOS, paper/shadow, documented approvals, new version | Canonical §116 and existing offline validation/promotion docs | Incomplete; no strategy promotion |
| 6. Production integration and safe operations | Broker-specific entry/stop/target, cancellation and partial-fill semantics, reconciliation, live system safety, operational checks, release promotion | Broker contract remains undefined; branch protection [#26](https://github.com/mlookhere/a-trade/issues/26) | Release blocked; no live execution approval |

### Known blocking or conditional work

- **[#26 — GitHub protection](https://github.com/mlookhere/a-trade/issues/26):** enforce and verify PR-only, required-check and no-force-push controls for `dev` and `main` before a release. Do not treat an inaccessible branch-protection API response as evidence that protection is active.
- **[#66 — Single-underlying Schwab sharding](https://github.com/mlookhere/a-trade/issues/66):** conditional capacity expansion. Implement only if measured provider capacity and independent streamer identity justify deterministic per-contract sharding; never evade provider restrictions.
- **Unresolved canonical qualitative/adapter rules:** consult the current list in [Issue #1](https://github.com/mlookhere/a-trade/issues/1) rather than inventing a threshold, extraction algorithm, provider behavior, or broker fill model.

## GitHub delivery protocol

**One deliverable = one Issue = one `work/<issue-number>-<slug>` branch from `dev` = one PR into `dev`.** `dev` integrates; `main` is production and receives only reviewed release promotion. Never commit directly to `dev` or `main` after seeding. Resuming an existing Issue/branch/PR takes precedence over creating more.

Every PR body references its Issue and contains **Result, Implementation, Verification, Risk, Remaining work**. Add `type:*` and `state:*` Issue labels and relevant `risk:*` labels. Keep changes narrow. Update the living control Issue after a verified merge; never close an issue merely because code was proposed.

**Required exact-head PR gates:** PR metadata, Fast deterministic gates (format/lint), PR test/build gates (locked tests/build), Dependency audit, and Workflow policy (security/workflow enforcement). Each must have completed successfully on the current PR head; **zero, missing, queued, running, cancelled or failing checks are not green**. Read the failing job log, fix the first cause, push and re-check; do not weaken thresholds or skip tests. After three failed attempts at the same cause, escalate for human input.

## Continuation procedure

1. Read this roadmap, [control Issue #1](https://github.com/mlookhere/a-trade/issues/1), current open Issues/PRs, `dev`/`main`/work branch heads and latest Actions. Trust GitHub's current state over this snapshot.
2. Resume the existing highest-priority unmerged deliverable. Use the Project Source router before any strategy-dependent code or tests. Preserve research vs production provenance and `UNKNOWN` fail-closed behavior.
3. Inspect the branch diff and exact-head checks. Fix real failures at their source. Merge only after **all five** required gates succeed.
4. Update the corresponding Issue and control checkpoint, including verified commit/PR/CI evidence. Continue to the next independently deliverable dependency.
5. Never claim production readiness without broker, risk, integration, historical/OOS, shadow, approval, versioning, and repository-release controls all independently verified.

**No implied approval:** Neither this roadmap nor a successful CI pipeline confers live trading authority.
