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

## Verified state and remaining dependencies — 2026-10-28 PDT

- `main` is the public repository's production/default branch; `dev` is integration and currently at `c88de8395a3981354b3080570712600c64f0d191`. Neither branch has verified protection; [#26](https://github.com/mlookhere/a-trade/issues/26) is a release blocker. Do not change the default branch.
- [#191](https://github.com/mlookhere/a-trade/issues/191) / [PR #192](https://github.com/mlookhere/a-trade/pull/192) was verified and squash-merged at that `dev` head. Exact PR head `1f7d4d6e2816b37499110fd8886b42e0019842d1` passed **5/5** required Actions jobs ([PR](https://github.com/mlookhere/a-trade/actions/runs/40822413393), [workflow policy](https://github.com/mlookhere/a-trade/actions/runs/40822413403)). Its shared account registry remains **offline only**, and is neither authenticated broker truth nor durable multi-process risk authority.
- Parent [#164](https://github.com/mlookhere/a-trade/issues/164) tracks the still-incomplete broker-grounded, atomic and unavoidable risk authority. [#66](https://github.com/mlookhere/a-trade/issues/66) remains **conditional** on verified independent Schwab streamer constraints; do not implement speculatively.
- Current deterministic CI functions through the legacy `.claude-workflow.json` command manifest. The configuration presently runs Rust/Python tools rather than Claude; nevertheless the legacy executable contract must be migrated to reviewed ChatGPT-native, runtime-neutral assets from `mlookhere/CI-Pipline` through [#194](https://github.com/mlookhere/a-trade/issues/194). No Claude agents, hooks, credentials or toolchain are permitted.
- **Live authorization remains FALSE.** No broker-specific fill/stop/cancel lifecycle, source-authenticated portfolio, durable cross-process risk reservation, complete empirical validation, paper/shadow approval, or protected production promotion has been demonstrated.

### Common delivery gate and risk policy

For every **new** numbered slice, use exactly one linked Issue, a `work/<issue>-<slug>` branch from `dev`, and a PR into `dev`. The PR requires all five **current-head** Actions checks completed successfully: **PR metadata; Fast deterministic gates; PR test/build gates; Dependency audit; Workflow policy**. Additionally verify the slice-specific headless commands/tests below. A successful historical PR is not evidence for another PR head. No skipped/missing/pending/red required check passes. PRs must reference their Issue and include Result, Implementation, Verification, Risk, Remaining work.

## Numbered CI-verifiable delivery plan

### 1. Scaffold, toolchain and baseline deterministic PR gates — [Issue #2](https://github.com/mlookhere/a-trade/issues/2) (historically completed)

**Acceptance:** Rust workspace, initial deterministic `auction-core` foundation, `dev`/ `main` boundaries, headless `./scripts/bootstrap` and `./ci/run` entry points, fast and PR gates. Preserve baseline instead of rebuilding. Current template/consumer parity must be revalidated in item 3; historical Issue closure alone is not live authorization.

**Areas:** `Cargo.toml`, `rust-toolchain.toml`, `crates/auction-core/`, `ci/`, `scripts/`, `.github/workflows/`.

**Required gates:** fast, PR and policy/security via all five current-head PR checks for any subsequent scaffold change. **Labels:** `type:feature`, `risk:high` (historical risk classification; do not relabel completed Issue).

### 2. Account-scoped process registry integration proof — [Issue #191](https://github.com/mlookhere/a-trade/issues/191) (completed)

**Acceptance:** mock/offline-only shared account registry connection with typed error provenance, competing-coordinator tests, wrong account, later UNKNOWN evidence and unsafe broker cases; no live routing authority, broker authentication, durable transaction or reservation release. [PR #192](https://github.com/mlookhere/a-trade/pull/192) was merged after five exact-head green checks.

**Areas:** `crates/auction-core/src/broker_execution.rs`, tests, `docs/BROKER_PORTFOLIO_EVIDENCE.md`.

**Required gates:** five checks, verified above; exact-head Rust tests. **Labels:** `type:hardening`, `risk:high`.

### 3. ChatGPT-native CI and toolchain manifest migration — [Issue #194](https://github.com/mlookhere/a-trade/issues/194) (next engineering slice)

**Acceptance:** review `mlookhere/CI-Pipline` and migrate only compatible runtime-neutral assets. Use `.chatfreept/project.json` and `docs/CHATFREEPT_CI.md` in place of legacy `.claude-workflow.json`; no Claude-specific agents, hooks, CLIs, policy dependencies or secrets. Preserve `dev`/ `main`, release/nightly PR job names, Rustfmt/Clippy/locked test/build/RustSec security commands, quality thresholds and audit evidence. Make missing/empty stage execution fail closed and verify parity before deletion of old manifest.

**Areas:** `.chatfreept/`, `ci/`, `scripts/`, `workflow/`, `.github/workflows/`, `.gitignore`, `README.md`, `docs/CHATFREEPT_CI.md`.

**Required gates:** `./scripts/bootstrap --ci`, `./ci/run fast`, `./ci/run pr`, `./ci/run audit`, manifest self-tests, five current-head Actions jobs. **Labels:** `type:chore`, `risk:high`.

### 4. Source-authenticated account, position and pending-order evidence — [Issue #195](https://github.com/mlookhere/a-trade/issues/195)

**Acceptance:** provider-authenticated and account-scoped snapshots; validated complete open/pending orders and positions, freshness, timestamps and source identity; fail closed on UNKNOWN, partial, duplicate, conflicting or mismatched evidence. Provider behavior must be verified from its real contract; if missing, only non-authorizing adapter scaffolding/tests may be delivered. Explicitly preserve parent [#164](https://github.com/mlookhere/a-trade/issues/164).

**Areas:** `crates/auction-core/` broker evidence, provider adapter boundary and tests, `docs/BROKER_PORTFOLIO_EVIDENCE.md`.

**Required gates:** broker evidence positive/negative fixtures, stale/UNKNOWN/wrong-account tests and all five Actions jobs. **Labels:** `type:hardening`, `risk:high`.

### 5. Durable cross-process atomic risk reservations — [Issue #196](https://github.com/mlookhere/a-trade/issues/196)

**Acceptance:** transactional account/cluster risk budget across independent processes with no duplicate SETUP_ID spend, restart data loss, missing broker exposure, stale snapshot acceptance or silent reservation release. Failover/crash and concurrency proof; do not invent provider TTL, fill or cancellation rules. Block live use until provider-grounded authority is verified.

**Areas:** `crates/auction-core/` risk and portfolio coordination, durable-store contract, race/restart tests, `docs/ATOMIC_PORTFOLIO_RESERVATIONS.md`.

**Required gates:** deterministic cross-process contention/crash tests, boundary/UNKNOWN/correlation tests, five Actions jobs. **Labels:** `type:hardening`, `risk:high`.

### 6. Non-bypassable execution gate and verified broker lifecycle — [Issue #197](https://github.com/mlookhere/a-trade/issues/197)

**Acceptance:** one unavoidable Strategy Validator → Risk Engine → Portfolio Coordinator → Execution Gate proof for each broker order; no legacy transmit bypass. Verified broker stop/bracket, acknowledgment, cancel/replace, fill/partial fill, unknown status, restart and terminal risk-release behavior. Unspecified broker semantics remain blocked, never guessed. Test post-11:00 entry denial and uninterrupted safe position management.

**Areas:** `crates/auction-core/` execution/portfolio/order state, broker-specific adapter once verified, integration and fault-injection tests.

**Required gates:** safety and negative-path broker contract tests, `UNKNOWN = NO_TRADE`, five Actions jobs. **Labels:** `type:hardening`, `risk:high`.

### 7. Advisory specialist orchestration and sealed authority — [Issue #198](https://github.com/mlookhere/a-trade/issues/198)

**Acceptance:** narrow, typed advisory specialists with immutable observations and snapshot/SETUP_ID lineage; never give an LLM final authority over time, inputs, state sequence, sizing, portfolio or orders. Isolate AI-native GEX research from canonical authorization; verify stale/mismatched/partial/duplicated agent output cannot pass. No new unapproved production threshold or strategy state.

**Areas:** `crates/auction-core/`, isolated `crates/gex-strategy/`, `prompts/`, `docs/ADVISORY_LLM_BOUNDARY.md`, contract tests.

**Required gates:** adversarial and fail-closed orchestration tests, replay validity, five Actions jobs. **Labels:** `type:feature`, `risk:medium`.

### 8. Reproducible full-system test, out-of-sample and paper/shadow proof — [Issue #199](https://github.com/mlookhere/a-trade/issues/199)

**Acceptance:** reproduce full canonical ordered scenarios and rejections, evaluate separate held-out data, compare research only as research, and record broker-safe paper/shadow evidence. Document §116 hypothesis → backtest → out-of-sample → paper/shadow → human approval → new version; no claim of profitable/approved/live behavior from an empty dataset or a green build.

**Areas:** replay fixtures, audit/metrics and report tools, offline test harness, `docs/OFFLINE_VALIDATION.md`, `docs/STRATEGY_CHANGE_PROMOTION.md`.

**Required gates:** deterministically reproducible scenario tests including invalid time, news, sequence, stop, broker, duplicates and correlation; five Actions jobs. **Labels:** `type:feature`, `risk:high`.

### 9. Production protection, release gates and approved promotion — [Issue #26](https://github.com/mlookhere/a-trade/issues/26) (blocked; pre-release)

**Acceptance:** enforce PR-only protected `dev` and `main`, no force push, verified required checks, no zero-check bypass; release PR from `dev` to `main` only with actual broker/replay/shadow evidence, §116 human approval and versioned artifact. GitHub settings capability not proven by a code change or inaccessible branch-protection endpoint: keep #26 open until externally verified.

**Areas:** repository branch protections/rulesets, `.github/workflows/ci-release.yml`, `docs/STRATEGY_CHANGE_PROMOTION.md`, release documentation.

**Required gates:** five `dev` PR jobs for code changes; release metadata, full regression/production build and workflow-policy jobs on exact release head; independently verified branch protection. **Labels:** `type:chore`, `risk:high`.

### Dependencies and stopping conditions

Items 4–6 require a verified actual broker/provider integration contract; do not fabricate implementation details to satisfy a roadmap. Item 8 needs independent market/replay evidence; item 9 needs human approval and GitHub protection. Items may be developed in narrow dependency-respecting PRs. Existing child Issues and conditional [#66](https://github.com/mlookhere/a-trade/issues/66) are not recreated. All unresolved requirements retain `TRADE_ALLOWED = FALSE` and prevent live trading.

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
