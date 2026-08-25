# AI-Native Ninja GEX vNext

Status: research / validation-only strategy surface. Not connected to live broker authorization.

## Source identity

User-approved source: `AI_NATIVE_NINJA_GEX_MULTI_AGENT_TRADING_SPEC.md`

SHA-256 of the supplied source used for this implementation unit:

`06d343aeef1edde17e5bc3a740ca24be5b91c1ca62e83194d3ebd2d6d8c5d310`

Primary sections used by Issue #57: §§0, 2, 5-11, 19-23, 28-30, 34, 36-42.

## Authority boundary

The user has approved the AI-native Ninja GEX specification as the target core strategy. The existing project knowledge still requires versioned validation before a materially changed production strategy is activated. Therefore this crate is intentionally isolated from the current live auction authorization/execution path until promotion completes.

This preserves both requirements:

- implement the approved GEX strategy as the target architecture;
- do not silently mutate an already-live strategy without validation and version promotion.

## Implemented in this unit

`crates/gex-strategy` provides:

- `StrictSourceBaseline` and `AiNative` operating-mode identities;
- immutable shared market snapshot identity and timestamp validation;
- structured/parsed/vision/unknown data-quality identity;
- signed GEX normalization;
- magnitude and distance-from-spot calculation;
- above/below/at-spot classification;
- aggregate versus expiry-specific GEX scopes;
- deterministic within-scope ranking only;
- GEX level-role, interaction-state, playbook, regime, and decision enums;
- a research-only gamma-regime classifier with no default window or mixed threshold.

## Research classifier provenance

The AI-native source requires an explicit `gex_regime_window` and `mixed_gamma_threshold`, but it does not define the exact mixed-regime formula.

Issue #57 therefore implements one deterministic research formalization:

1. select exactly one GEX scope;
2. include levels inside the configured distance window;
3. sum positive and negative absolute GEX separately;
4. calculate the smaller side's share of total absolute GEX;
5. classify `MIXED` when that share is at least the explicitly supplied threshold;
6. otherwise classify by the dominant sign;
7. return `UNKNOWN` when no non-zero usable GEX exists in the selected scope/window.

This formula is tagged `ValidationRequiredAssumption`. It has no production default and MUST NOT authorize live trades until the project's validation/promotion sequence approves it.

## Invariants preserved

- signed numeric GEX outranks UI color interpretation;
- aggregate and expiry-specific GEX are never ranked together;
- level magnitude alone never assigns support/resistance/magnet/trap-door roles;
- missing or invalid required data fails closed;
- no GEX value can directly submit an order;
- deterministic risk and execution remain final authorities;
- no raw LLM confidence is treated as calibrated win probability.

## Next units

After Issue #57 passes CI and merges:

1. finish the existing live-entry cutoff hardening path;
2. add audited third-party Rust dependency support (#55);
3. add Schwab stream-first GEX transport (#56);
4. build explicit hard-veto contracts and playbook eligibility for vNext;
5. add specialists/adversarial review/arbiter only after deterministic snapshot and feature inputs are stable;
6. validate AI-native regime/playbook behavior through backtest, OOS, paper/shadow, approval, and version promotion before live activation.
