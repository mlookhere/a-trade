# a-trade

Low-latency multi-agent trading-system development built around deterministic authority, fail-closed execution, and versioned strategy research.

## Project north star

The system is designed to give narrow specialist agents more observation bandwidth than one human trader while keeping one controlled authority path for every order. Parallel agents may interpret different evidence domains, but they do not vote an order into existence and cannot override deterministic safety, risk, portfolio, or execution gates.

## Authority lanes

The repository intentionally separates three concerns.

### 1. Canonical auction strategy — `auction-core`

`auction-core` implements the canonical 123-section Multi-Agent First-90-Minute Auction Trading System. Repository code implements that knowledge; it does not redefine it. `strategy/authority.json` records the canonical fingerprint and implementation provenance.

Canonical setup progression remains:

`environment -> location -> participation -> effort versus result -> absorption -> dominance shift -> second attempt -> second failure -> reconfirmation -> risk validation -> execution`

`TRADE_ALLOWED` begins false. Missing, stale, conflicting, partial, invalid, duplicated, or `UNKNOWN` required state fails closed. New entries are limited to 09:30:00 through 10:59:59 ET, and the live execution boundary rechecks time immediately before broker transmission. Existing-position management remains separate after the cutoff.

### 2. Provider and deterministic data infrastructure

`gex-engine` and provider adapters supply validated low-latency evidence. They have no strategy, direction, sizing, risk, or broker-order authority.

The GEX engine maintains provider-neutral per-contract, per-strike, per-expiration, and aggregate exposure analytics. Side-signed open-interest GEX is an analytical model, not a claim of actual dealer inventory. Gamma context never becomes a standalone canonical entry signal.

Issue #56 / PR #64 added the first Schwab stream-first GEX market-data adapter. It supports arbitrary-count runtime API profiles with independent credentials, token stores, REST budgets, bounded transport/reconnect behavior, and streamer identities while preserving one authoritative provider state per tracked underlying. Its always-on supervisor reconnects and resubscribes retryable stream failures without treating reconnect as a reason to start a REST bootstrap storm. Provider-side limits remain Schwab's authority. Contract-level sharding of one underlying across independent profiles remains separate follow-up Issue #66.

### 3. AI-native GEX vNext research — `gex-strategy`

`gex-strategy` is the isolated implementation surface for the approved AI-native Ninja GEX specification. It keeps `STRICT_SOURCE_BASELINE` available for comparison while developing the `AI_NATIVE` architecture around immutable snapshots, deterministic features, gamma-regime/playbook compatibility, specialist evidence, adversarial review, calibrated expected value, and deterministic risk/execution.

This lane does not silently mutate the canonical auction strategy or connect itself to live broker authorization. Any production strategy change still requires the versioned promotion sequence:

`hypothesis -> backtest -> out-of-sample -> paper/shadow -> human approval -> new version`

## Runtime authority model

```text
MARKET / OPTIONS / BROKER DATA
            |
            v
DATA VALIDATION + NORMALIZATION
            |
            v
DETERMINISTIC FEATURES / STATE
            |
            v
SPECIALIST AGENTS IN PARALLEL
            |
            v
STRATEGY VALIDATION / ARBITRATION
            |
            v
DETERMINISTIC RISK ENGINE
            |
            v
PORTFOLIO COORDINATION
            |
            v
DETERMINISTIC EXECUTION GATE
            |
            v
BROKER
            |
            v
POSITION MANAGEMENT + AUDIT
```

For the canonical auction path, LLM output remains advisory and the Strategy Validator, Risk Engine, Portfolio Coordinator, broker-safety checks, and Execution Gate retain final authority. For AI-native vNext, specialist evidence is reconciled hierarchically rather than by majority vote, and deterministic hard veto/risk/execution authority remains non-overridable.

## Engineering split

- **Rust:** latency-critical market/provider state, deterministic calculations, strategy/risk/execution authority, setup ownership/state, portfolio coordination, broker-safety primitives, and audit/replay contracts.
- **LLM specialists:** narrow interpretation of ambiguous evidence and adversarial review. Numeric features should be calculated once and shared rather than recomputed by multiple agents.
- **Python/offline tooling:** replay, research, calibration, evaluation, analytics, and CI support; not the latency-critical live authority path.

The codebase follows DRY, KISS, and YAGNI: precompute deterministic features once, parallelize genuinely independent domains, use compact shared snapshots, fail fast on hard vetoes, and do not create agents or abstractions without an evidence-backed need.

## Development workflow

Production branch: `main`  
Integration branch: `dev`

Each independently deliverable change follows:

`Issue -> work/<issue>-slug -> PR -> dev`

Hosted gates route through `./ci/run <stage>` using `.claude-workflow.json`. PR/release/nightly policy includes formatting, linting, locked workspace tests/builds, dependency auditing, and workflow-policy validation.

`main` is not currently a promoted live trading release. Current development remains pre-production: the canonical deterministic core and Schwab market-data transport exist, but production LLM orchestration, historical/OOS evidence, paper/shadow evidence, an exact real broker execution adapter, final release promotion, and live authorization remain incomplete.

## Current hardening focus

Issue #65 / PR #67 is the repository-wide hardening/context pass. It reconciles documentation and canonical provenance with the merged Schwab transport, audits unresolved provider and strategy boundaries, preserves the three authority lanes, and keeps unsupported behavior fail closed. Issue #26 remains a release blocker because `dev` and `main` are currently unprotected. Issue #66 remains isolated capacity work rather than being folded into provider transport without evidence that it is required.
