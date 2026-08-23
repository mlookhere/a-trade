# a-trade

Low-latency, multi-agent implementation of the First-90-Minute Auction Trading System.

## Authority

Strategy behavior is governed by the project's canonical 123-section knowledge set. Repository code implements that knowledge; it does not redefine it. `strategy/authority.json` records the canonical fingerprint and the sections implemented by each development slice.

## Runtime split

- **Rust:** production market state, deterministic strategy rules, setup ownership/state, risk, portfolio coordination, broker safety, execution, and position-management primitives.
- **Local LLM:** narrow auction interpretation only; advisory until deterministic gates pass.
- **Python:** offline replay, research, calibration, evaluation, analytics, and CI tooling. Python is not part of the latency-critical execution path.

## Canonical sequence

`environment -> location -> participation -> effort versus result -> absorption -> dominance shift -> second attempt -> second failure -> reconfirmation -> risk validation -> execution`

`TRADE_ALLOWED` starts false. Missing, stale, conflicting, partial, or `UNKNOWN` required state fails closed.

## Development workflow

Production is `main`; integration is `dev`.

Each independently deliverable change follows:

`Issue -> work/<issue>-slug -> PR -> dev`

All hosted gates route through `./ci/run <stage>` using `.claude-workflow.json` as the central stage/command configuration.

Current phase: deterministic core foundation only. No broker, market-data, model, paper, or live execution path exists yet.
