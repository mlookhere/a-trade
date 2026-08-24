# Required Data and Frozen Premarket Contract

Governing canonical sections: §§11, 13, 16, 17, 25, 33, 108, 109, 122.

## Required data

`RequiredMarketDataStatus` represents availability/validity of the exact §11 market-data families: current bid, current ask, last trade, and 1m/5m/15m/1h/4h OHLCV. `RequiredVolumeProfileStatus` represents the exact §13 profile families plus explicit facts that the same methodology is used across sessions and is not changed intraday.

These are tri-state facts. Any FALSE or UNKNOWN result fails closed. The core does not define provider-specific freshness durations, bar construction, profile formulas, node-detection algorithms, or data-source behavior that canonical knowledge does not specify.

`DataCycleReadiness` composes those required inputs with the existing §16 freshness/broker-state `DataHealth` contract. It produces only a deterministic `Condition` suitable for the existing `DATA_VALID` authorization fact.

## Premarket plan

`PremarketPlanComponents` represents the exact §17 concepts that must be established before an instrument can trade: market environment, direction permission, relevant swing structure, Fib location, gamma regime, structural targets, and invalidations. Reference value is enforced through the locked §25 `PremarketReferences` supplied to the frozen scenario.

`FrozenPremarketScenario::freeze` requires:

- nonempty agent, instrument, and scenario identity;
- structurally valid locked §25 references;
- every §17 component TRUE;
- freeze time strictly before 09:30:00 ET.

A successful frozen record exposes `PREMARKET_PLAN_COMPLETE = TRUE`. FALSE or UNKNOWN premarket components cannot create the record. The object has no strategy-parameter mutation, order construction, broker transmission, risk override, or live execution API.

## §33 example handling

The field layout shown in canonical §33 is an example, not an exact machine schema. The Rust core therefore stores an opaque scenario identity rather than hard-coding the example into a universal scenario DSL. Upstream scenario content can evolve only within its own validated contract; this module proves that a complete scenario was frozen before the opening move.
