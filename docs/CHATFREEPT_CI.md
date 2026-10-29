# ChatGPT-native CI operations — a-trade

## Source of truth

ChatGPT, using the linked GitHub MCP connector, is the only autonomous development agent. GitHub Issues, branches, PRs and exact-head Actions jobs are durable development evidence. There is no Claude, Claude Code, runner-side coding agent, LLM API dependency or agent credential in CI.

- Integration branch: `dev`; production/default: `main`. Do not force-push or directly commit to either after initial seed.
- One Issue → one `work/<issue>-slug` branch → one PR into `dev`. The PR body includes Issue, Result, Implementation, Verification, Risk, Remaining work.
- Every PR requires **five completed-success checks on its current head**: PR metadata, Fast deterministic gates, PR test/build gates, Dependency audit and Workflow policy. Zero, missing, pending, skipped or red checks block merging.
- `main` promotion is a separate reviewed PR with Release metadata, Full regression and production build, and Workflow policy. Branch protection remains an outstanding release prerequisite tracked by [#26](https://github.com/mlookhere/a-trade/issues/26).

## Deterministic execution

`.chatfreept/project.json` is the repository/branch/agent/required-check contract. `.chatfreept/ci.json` is the command and quality-stage contract. `ci/run.py`, `ci/quality.py`, and `workflow/self_test.py` read these manifests directly. The self-test locks the previous Rust/Python command matrix, five PR job names, three release job names, quality thresholds, action pinning, audit-cache ordering, and canonical strategy fingerprint. Missing or invalid manifests fail closed.

~~~bash
./scripts/bootstrap --ci
./ci/run fast
./ci/run pr
./ci/run audit
~~~

The same CI runner supports `./ci/run release` and `./ci/run nightly`. The locked checks remain unchanged: Rustfmt; Clippy with denied warnings; locked workspace tests plus gex-strategy doctests; locked release workspace build; Python workflow/quality checks; and the RustSec dependency audit. GitHub-hosted Actions results, not a locally successful command, determine merge eligibility.

## Migration inventory

Reviewed the upstream `mlookhere/CI-Pipline` runtime-neutral control-plane design. The upstream `.chatfreept/ci.json` runs Python-specific pytest/mypy/ruff/pip-audit commands, which cannot replace the existing Rust workspace gates. Therefore this consumer retains its exact headless Rust/Python commands, pinned workflows, audit cache policy, `.gitattributes`, and existing `.gitignore`; there is no need to import unrelated Python dependencies, pre-commit hooks or project-specific template automation. No upstream Claude-specific hooks, skills, scripts, workflows or credentials are vendored.

This migration removes the legacy *executable* root configuration after changing its three consumers. The old filename did not itself execute Claude, but retaining it would leave an obsolete agent-specific control surface. This work is an implementation-only CI change and does not modify strategy logic.

## Trading safety

This repository contains a deterministic auction-core **production candidate**, not an approved live trading system. CI does not grant broker execution authority. `TRADE_ALLOWED` defaults FALSE; canonical `UNKNOWN = NO_TRADE`; risk and order routing require the full validated gate sequence. Outstanding [#164](https://github.com/mlookhere/a-trade/issues/164) covers authenticated/durable portfolio authorization. Live release also requires the canonical §116 hypothesis → backtest → out-of-sample → paper/shadow → human approval → new version process.
