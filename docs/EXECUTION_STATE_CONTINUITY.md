# Execution-tail state continuity

Canonical basis: §§64, 102, 103, 109, 117, 118.

The generic canonical state machine already defines the full setup sequence. This change does not create a second state machine. Instead, the existing execution setup registry now persists the live tail of that canonical sequence once a sealed deterministic proposal reaches the Execution Gate.

The deterministic live transitions are:

```text
FINAL_RECONFIRMATION
→ ENTRY_AUTHORIZED
→ ORDER_PENDING
→ FILLED
→ POSITION_MANAGEMENT
```

A successful Execution Gate binds the setup owner at `ENTRY_AUTHORIZED`. Broker acceptance advances to `ORDER_PENDING`. A confirmed fill advances to `FILLED` before the setup is marked consumed. Confirmed hard-stop protection advances the filled setup to `POSITION_MANAGEMENT`, after which the §118 open-position safety cycle is permitted.

A pre-transmission safety failure may clear the order reservation while retaining `ENTRY_AUTHORIZED`; state does not regress. The same owner can re-enter the gate only while there is no active broker order and all fresh gates pass again.

If an entry fills but hard-stop protection cannot be confirmed, canonical §64 still requires immediate flatten and execution failure. The registry remains at `FILLED` because the project knowledge does not specify which alternate terminal state should represent that execution failure. No terminal state is invented.

`CLOSED`, provider-specific cancellation, partial-fill behavior, broker retry policy, and polling cadence remain outside this unit until their exact semantics are specified.
