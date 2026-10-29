# Open-position broker and stop safety

Canonical basis: §§16, 64, 73, 98, 100, 101, 118.

`ExecutionCoordinator::verify_open_position_safety` is the provider-neutral safety check used while a position is open. It accepts only a filled/consumed registered setup, requires current broker/position/open-order state to be known, requires the reconciled broker position to contain the same `SETUP_ID`, and reconfirms hard-stop protection.

A missing, `UNKNOWN`, or unqueryable hard stop reuses the canonical protected-entry failure behavior: one immediate flatten attempt, an execution-unsafe latch, and a broker-unsafe failure. Broker-state uncertainty or a missing reconciled position fails closed but is not converted into invented provider-specific recovery behavior.

This boundary does not define polling frequency, broker SDK behavior, retries, cancel/replace semantics, partial fills, or deployment scheduling. Those remain outside the strategy core until explicitly specified.

A latched shared execution failure continues to block **all new entries**, but it does not suppress read-only broker reconciliation and protective hard-stop verification for a position already in `POSITION_MANAGEMENT` (§§73, 101, 118). A missing stop still triggers an immediate protective flatten attempt, recorded once per consumed setup within the current in-memory coordinator. Rechecking does not clear the unsafe latch or retry the flatten. This does not establish durable crash recovery, provider-specific cancellation, transport idempotency, or a new live trading permission.
