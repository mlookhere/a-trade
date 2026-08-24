# Deterministic authority proof boundary

Governing knowledge: canonical §§6, 56, 59-72, 93-94, 98, 100, 103, 109-110, 117-119, 122.

## Boundary

`StrategyValidationProof` is created only by the deterministic Strategy Validator after the canonical pre-risk sequence passes. The proof carries setup identity, owner, instrument, direction, and the accepted first-failure/final-reconfirmation evidence required by the downstream risk/order stage. Callers cannot construct the proof or replace those facts with raw pass booleans.

`build_order_proposal` consumes that proof and deterministically recalculates the entry trigger, structural stop, structural-target validity, futures position size, aggregate portfolio risk, cluster exposure, and opposing-cluster conflict. The resulting `OrderProposal` is sealed outside the crate. Strategy, risk, and portfolio authority are TRUE only when the deterministic builder succeeds; execution authority remains UNKNOWN.

The live `ExecutionCoordinator` accepts the sealed proposal plus only advisory `LLM_SETUP_PASS`. It independently checks operational safety, setup ownership/single consumption, broker protected-bracket capability, broker/position/open-order reconciliation, conflicting-order state, exact SETUP_ID clearance, and execution-engine safety before creating an opaque execution permit. FALSE or UNKNOWN fails closed.

## Risk semantics preserved

Canonical §66 defines futures risk using `ABS(ENTRY - STOP) / TICK_SIZE` and separately includes `SLIPPAGE_RESERVE` in `RISK_PER_CONTRACT`. Canonical §§59-60 also distinguish the stop trigger from the instrument-configured STOP_LIMIT slippage allowance, but the source does not define an alternate sizing formula that substitutes the STOP_LIMIT limit price for `ENTRY`. This authority-hardening change therefore preserves the existing deterministic sizing interpretation and explicit externally supplied slippage reserve rather than silently changing production risk semantics.

MNQ four-tick maximum entry slippage (§60), MNQ two-tick stop buffer (§62), 0.25% risk (§65), and cluster/portfolio ceilings (§§67-69) remain example-only or externally configured exactly as already classified. The strict autonomous `PLANNED_R >= 1.5` rule remains the existing §72 implementation and is not promoted into a universal source-trader rule.

## Execution safety

A broker submission must use a protected ENTRY + HARD STOP + TARGET structure where supported (§64). A confirmed fill without confirmed hard-stop protection triggers immediate flatten and marks the shared execution engine unsafe. Duplicate setup ownership and broker conflicts cannot be overridden by an LLM or caller-supplied TRUE condition.

## Explicitly unresolved / out of scope

No provider-specific retry, partial-fill, replace, cancel, routing, or continuous open-position reconciliation semantics are invented here. No historical-data adapter, live deployment policy, or autonomous strategy modification is added. Unsupported or UNKNOWN states remain NO_TRADE.
