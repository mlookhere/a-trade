# Advisory LLM Boundary

This repository treats LLM output as advisory only.

The canonical multi-agent master prompt is stored verbatim in `prompts/agent_master_prompt_canonical.txt` from canonical §121. Strategy thresholds are not duplicated into provider prompts or SDK configuration.

`auction_core::AdvisorySetupEvaluation` is the typed LLM-facing setup evaluation contract. Mandatory conditions use `Condition::{True, False, Unknown}` under canonical §108. Missing, invalid, or structurally unsafe advisory output remains fail-closed.

The advisory boundary may produce only `llm_setup_pass()`. It cannot create an `ExecutionPermit`, submit through `BrokerAdapter`, size a position, override portfolio risk, mutate live strategy parameters, or bypass the deterministic authorization chain in §§109 and 122.

`trade_allowed` is retained as a §107-shaped field but must remain `false` on LLM-facing output. An LLM that returns `trade_allowed = true` has crossed its authority boundary; validation rejects it and `llm_setup_pass()` returns `UNKNOWN`.

Provider/model integration is intentionally absent. Adding a local model adapter later must preserve this boundary and should not move deterministic strategy logic into prompts.
