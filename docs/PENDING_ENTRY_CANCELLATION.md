# Pending Entry Cancellation Boundary

Canonical basis: §§60-61, 98, 100, 102, 117.

An accepted STOP_LIMIT entry may remain `ORDER_PENDING` only while the broker still reports it unfilled, current price remains inside the sealed instrument-specific slippage allowance, and fewer than two completed 5-minute candles have elapsed since reconfirmation.

`OrderProposal` retains the deployment-owned `InstrumentExecutionConfig` used when the proposal was built. The live coordinator binds direction, trigger, and execution configuration from that sealed proposal; callers cannot replace those facts during pending reconciliation.

`reconcile_pending_entry` checks fill state before any cancellation decision. A confirmed fill follows the existing `ORDER_PENDING -> FILLED -> POSITION_MANAGEMENT` protected-entry path and cannot be converted into a missed trade because price or age inputs would otherwise require cancellation.

If the broker explicitly reports the entry unfilled and either the §60 slippage guard fails or §61 expiry is reached, the cancellation-capable broker adapter must confirm cancellation. Only `TRUE` cancellation confirmation terminates the setup as `MISSED`. The setup remains unconsumed and the same `SETUP_ID` cannot be reauthorized; a new setup sequence is required.

Cancellation `FALSE`, `UNKNOWN`, or adapter failure does not fabricate `MISSED`. The setup remains `ORDER_PENDING`, the execution engine is latched unsafe, and the caller receives `BROKER_UNSAFE`. Invalid price data likewise fails closed without claiming that a broker cancellation occurred.

`BrokerOrderCancellation` defines only the provider-neutral cancellation confirmation boundary. Broker SDK calls, retry policy, cancel/replace semantics, partial-fill handling, polling cadence, and recovery of an unsafe live order remain provider-specific and are not defined here.
