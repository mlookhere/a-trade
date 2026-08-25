# Full-system hardening status

Status: living development audit for Issue #65. This document records implementation state; it does not create or modify strategy rules.

## Authority model

The repository has three intentionally separate lanes:

| Lane | Current authority | Live/order authority |
| --- | --- | --- |
| `auction-core` | Canonical 123-section First-90-Minute Auction strategy | Production-candidate deterministic authority only; no real live broker adapter/release yet |
| `gex-engine` + provider adapters | Deterministic analytics/data infrastructure | None; may supply validated evidence only |
| `gex-strategy` | Approved AI-native Ninja GEX vNext/research specification | None until versioned validation/promotion |

Canonical production logic remains fail closed: `TRADE_ALLOWED` starts false; mandatory conditions use TRUE/FALSE/UNKNOWN; `UNKNOWN = NO_TRADE`; ordered setup states cannot be skipped; Strategy Validator, Risk Engine, Portfolio Coordinator, broker safety, operational safety, and Execution Gate remain non-bypassable.

The AI-native research lane uses its own explicit WAIT/REJECT/ALLOW contract but does not bypass deterministic hard veto/risk/execution authority. Its strategy changes cannot self-promote.

## Completed and merged verification matrix

| Area | Implementation state | Verification state | Remaining boundary |
| --- | --- | --- | --- |
| Canonical session/time permissions | Complete | Exact entry-window boundaries plus second pre-transmit check are contract-tested | Provider-accepted order behavior after cutoff remains broker-specific |
| Environment/value/Fib/location | Complete for specified deterministic rules | Canonical/data/environment/location contracts present and CI-validated | §22 substantial value-area overlap remains externally supplied/UNKNOWN when unresolved |
| Long order-flow sequence | Complete through FINAL_RECONFIRMATION | Long contract suite covers ordered sequence and invalid paths | §§45-46 meaningful progression and §52 first-failure extraction remain qualitative/externally supplied where canonical source is not algorithmic |
| Mirrored short sequence | Complete through FINAL_RECONFIRMATION | Mirrored short contracts are part of workspace CI | §§88-89/91 retain the corresponding unresolved qualitative/extraction boundaries |
| Setup state/ownership/dedup | Complete | State-tail, broker execution, duplicate/ownership tests | None beyond provider-specific broker semantics |
| Entry/stop/sizing/target | Complete for canonical/explicit formalizations | Order/risk contract tests and locked workspace CI | Deployment-configured values remain configuration; examples are not defaults |
| Portfolio/correlation risk | Complete provider-neutral core | Aggregate/cluster/conflict rejection tests | Real broker/account reconciliation adapter still absent |
| News/operational kill switches | Complete | News/hardening/operational negative-path tests | External schedules and emergency thresholds remain external configuration |
| Protected execution/open-position safety | Complete provider-neutral contracts | Broker execution, pending entry, state-tail, open-position safety suites | Broker-specific fill/cancel/replace/partial-fill/poll semantics remain unresolved |
| Position management | Complete where canonical behavior is deterministic | Long/short management and stop-safety tests | §118 structural-reclaim-failure and post-entry pivot extraction remain unresolved rather than inferred |
| Audit/replay/promotion | Complete infrastructure | Audit, replay/OOS and §116 ordering contracts | No real historical performance study or paper/shadow evidence yet |
| Canonical gamma context | Complete | Gamma contract proves no directional authority | Reliable provider gamma flip remains unavailable until separately defined/validated |
| High-speed `gex-engine` | Complete analytics foundation | Signed exposure, multiplier, incremental replacement/removal, spot rescale, walls, stale/invalid input tests | Side-signed OI is an inference; wall derivation is analytics; gamma flip unresolved |
| Rust dependency auditing | Complete | CI uses pinned fail-closed RustSec audit path | New dependencies must continue through this gate |
| AI-native GEX foundation | Complete only for the first foundation slice | Snapshot, normalization, scope-safe ranking, explicit modes/types and research regime tests | Full feature engine, hard vetoes, specialist ensemble, playbook eligibility/role logic, arbiter, calibrated EV and persistence are not implemented yet |

## Active Schwab adapter hardening (#56 / PR #64)

Implemented on the active branch:

- arbitrary-length runtime API profiles with separate credentials/token stores/REST budgets/streamer identities;
- explicit provider/deployment timeouts rather than unbounded HTTP/WSS operations;
- OAuth authorization/refresh and validated/redacted token persistence;
- REST option-chain bootstrap and streamer discovery;
- WSS ADMIN login, LEVELONE_OPTIONS and LEVELONE_EQUITIES requests;
- request-ID exhaustion fail close and ambiguous-frame rejection;
- one authoritative GEX state per underlying;
- partial stream-field merge without zero-filling missing fields;
- coverage/timestamp/freshness/delayed-state validation;
- sticky profile ownership, explicit failure release/reassignment, duplicate streamer-user prevention;
- provider-integrity errors latch affected runtime state uncertain until rebootstrap;
- recorded bootstrap/partial-update fixtures and multi-profile/rate/provider/runtime hardening contracts.

Not yet considered complete:

- final always-on streamer supervision/reconnect/resubscribe lifecycle is not yet proven end-to-end; the PR must either implement it or explicitly narrow its completion claim;
- the latest hardening head still must complete every hosted gate, including RustSec, after all changes;
- one large underlying is not contract-sharded across profiles in #56; that separate capacity problem is Issue #66 and must preserve one authoritative aggregate state.

## AI-native implementation directive status

The approved AI-native specification §42 is currently at this boundary:

| Directive | Status |
| --- | --- |
| 1. data model / deterministic feature engine | **Partial** — core market/GEX data model exists; broad trend/structure/flow/liquidity deterministic feature engine does not |
| 2. GEX normalization / regime | **Foundation complete** — normalization complete; current mixed-regime formula remains research-only `ValidationRequiredAssumption` |
| 3. immutable snapshot | **Foundation complete** |
| 4. hard veto/risk invariants | **Not implemented in `gex-strategy`**; canonical auction risk engine is separate and must not be silently reused as a different strategy rule set |
| 5. specialist interface | Not implemented |
| 6. parallel specialists | Not implemented |
| 7. playbook registry | Types exist; eligibility/selection engine not implemented |
| 8. adversarial review | Not implemented |
| 9. hierarchical arbiter | Not implemented |
| 10. calibrated EV interface | Not implemented; raw LLM confidence remains prohibited as P(win) |
| 11. deterministic execution validation | Not connected for vNext |
| 12. evaluation/outcome persistence | Not implemented for vNext |
| 13. strict source baseline | Mode identity exists; full 12-check observable/evaluation implementation is not complete |
| 14. AI_NATIVE extensible architecture | Foundation only |
| 15. provenance control | Present in foundation; must remain enforced in each future slice |

## Hardening findings that remain intentionally unresolved

Do not fill these gaps with convention or guesses:

- canonical §22 overlap threshold;
- canonical meaningful effort-versus-result progression thresholds where source is qualitative;
- canonical first-failure candle extraction algorithm;
- canonical post-entry structural pivot/breakeven permission algorithm;
- canonical §118 structural-reclaim-failure exit definition;
- broker-specific order/fill/cancel/replace/partial-fill semantics;
- AI-native GEX regime window, major-level threshold, near tolerance, mixed threshold, transition/acceptance rules, liquidity/slippage limits, risk limits, EV threshold, probability model, and option-selection rules until explicitly configured/validated;
- actual market-maker inventory: public side-signed-OI GEX remains an analytical inference.

## Ordered next steps

1. Finish #56 hardening and merge only on a fully green final head.
2. Rebase/finish #65 documentation and provenance alignment on the merged #56 state; verify the control Issue and README match repository reality.
3. Keep #66 separate and implement it only if single-underlying capacity requires cross-profile contract sharding and reliable cross-shard reconciliation can be proven.
4. Resume AI-native §42 at the first incomplete deterministic layer: broad feature engine plus explicit hard-veto contracts, not specialist LLM proliferation.
5. Add typed specialist output interfaces and parallel specialists only after snapshot/features are stable.
6. Then implement playbook eligibility/role/interaction logic, adversarial review, hierarchical arbiter, calibrated-EV interface, deterministic execution validation, and outcome persistence in that order.
7. Build replay/historical datasets to compare `STRICT_SOURCE_BASELINE` against `AI_NATIVE`; strategy promotion still requires complete §116 evidence.
8. Implement exact provider-specific broker execution/reconciliation contracts before any live order path or production release.
9. Re-audit GitHub branch/ruleset protection before release; current `dev` and `main` are not claimed protected.

## Completion definition

A component is not called complete merely because a type or happy path exists. It is complete only when its authority/provenance is correct, fail-closed negative paths are covered, deterministic gates cannot be bypassed, unresolved behavior stays unresolved, CI is green, and the component is not documented as having authority it does not actually possess.
