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
| Schwab GEX market-data adapter | Complete first transport foundation | Final PR #64 head passed metadata, fast, workspace/release, RustSec/dependency, and security/workflow gates before merge | Single-underlying contract sharding remains #66; provider limits remain external; gamma flip unresolved |
| AI-native GEX foundation | Complete only for the first foundation slice | Snapshot, normalization, scope-safe ranking, explicit modes/types and research regime tests | Full feature engine, hard vetoes, specialist ensemble, playbook eligibility/role logic, arbiter, calibrated EV and persistence are not implemented yet |

## Completed Schwab adapter foundation (#56 / PR #64)

Merged into `dev` as `d547a03a2675c42ffbe9ff8a5016e5e716f5c366` after the exact final PR head passed every hosted gate.

Implemented boundaries include:

- arbitrary-length runtime API profiles with separate credentials, token stores, REST budgets, reconnect delay, transport timeout, and streamer identities;
- OAuth authorization/refresh and validated/redacted token persistence;
- atomic token writes with restricted Unix temporary-file permissions;
- explicit HTTPS callback validation and explicit nonzero deployment/provider timeouts rather than hidden strategy defaults;
- REST option-chain bootstrap and streamer discovery;
- WSS ADMIN login plus LEVELONE_OPTIONS and LEVELONE_EQUITIES subscriptions;
- request-ID exhaustion fail close and ambiguous-frame rejection;
- one authoritative GEX state per underlying;
- partial stream-field merge without zero-filling absent fields;
- expected/hydrated coverage and option/underlying timestamp tracking;
- wrong-profile, stale, delayed, future, partial, malformed, conflicting, and unknown-contract state prevented from becoming reliable Gamma output;
- sticky profile ownership, explicit failure release/reassignment, and duplicate streamer-user prevention;
- provider-integrity errors latch affected state uncertain until explicit reconciliation/rebootstrap;
- an always-on supervisor for retryable disconnect/reconnect/resubscribe lifecycle;
- current streamer metadata rediscovery and subscription restoration on reconnect;
- reliable-live activation only after required subscription acknowledgements succeed, with intervening data buffered until activation;
- disconnect removes reliable-live state immediately but does not itself trigger REST option-chain bootstrap, avoiding reconnect-driven REST storms;
- provider connection-limit, symbol-limit, integrity, and other non-retryable provider failures remain fail closed rather than being looped indefinitely.

The transport remains evidence infrastructure only. It cannot choose direction, approve risk, authorize a setup, construct a canonical trade decision, or route a broker order. Canonical §§14-15 still make Gamma volatility context rather than a directional signal.

Remaining provider boundaries are explicit:

- Issue #66 owns any future contract-level sharding of one large underlying across genuinely independent streamer identities; it must preserve one authoritative aggregate state and prove cross-shard coverage/freshness/ownership before use;
- provider-side application, account, entitlement, request, connection, and symbol limits remain external authority and are not guessed or bypassed;
- public side-signed-OI GEX remains an analytical inference rather than actual market-maker inventory;
- gamma flip remains unresolved until a separately specified, validated, and approved model exists.

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

1. Finalize Issue #65 / PR #67 documentation and canonical provenance, including the completed #52 live-cutoff slice, and merge only after the exact final head is fully green.
2. Re-audit and enforce Issue #26 repository branch/ruleset protection before any release or promotion to `main`; current `dev` and `main` are not claimed protected.
3. Keep #66 separate and implement it only if single-underlying capacity proves cross-profile contract sharding is needed and reliable cross-shard reconciliation can be demonstrated.
4. Resume AI-native §42 at the first incomplete deterministic layer: broad feature engine plus explicit hard-veto contracts, not specialist LLM proliferation.
5. Add typed specialist interfaces and parallel specialists only after snapshot/features are stable.
6. Then implement playbook eligibility/role/interaction logic, adversarial review, hierarchical arbiter, calibrated-EV interface, deterministic execution validation, and outcome persistence in that order.
7. Build replay/historical datasets to compare `STRICT_SOURCE_BASELINE` against `AI_NATIVE`; strategy promotion still requires the complete canonical §116 evidence sequence.
8. Implement exact provider-specific broker execution/reconciliation contracts before any live order path or production release.

## Completion definition

A component is not called complete merely because a type or happy path exists. It is complete only when its authority/provenance is correct, fail-closed negative paths are covered, deterministic gates cannot be bypassed, unresolved behavior stays unresolved, CI is green, and the component is not documented as having authority it does not actually possess.
