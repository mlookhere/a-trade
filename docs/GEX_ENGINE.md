# GEX Engine Contract

Governing canonical sections: §§5-6, 14-15, 25, 33, 77, 83, 111, 117.

## Boundary

`gex-engine` is a provider-neutral analytics layer. It does not authorize trades, determine long or short direction, alter the canonical setup sequence, change risk, or submit orders. `auction-core` remains the strategy authority.

The engine exists to feed the Gamma Agent with low-latency context that can later be validated and surfaced as:

- net gamma regime;
- call-side and put-side exposure by strike;
- exposure by expiration;
- call-wall and put-wall analytics;
- timestamp/coverage metadata needed by the Data Supervisor.

Canonical §15 remains unchanged: positive gamma is dampened-volatility / higher-mean-reversion / lower-breakout-persistence context, while negative gamma is amplified-volatility / higher-move-acceleration context. Neither regime creates direction permission.

## Exposure model and provenance

The engine uses the implementation-level open-interest convention:

```text
coefficient = gamma × open_interest × multiplier × 0.01
GEX         = coefficient × spot²
```

Calls are signed positive and puts negative. The option multiplier is required from the provider; the engine does not assume every contract uses multiplier 100.

This is an inferred, side-signed open-interest exposure model. It is **not** a measurement of an actual market maker's inventory or hedge book. The formula and sign convention are analytics implementation choices, not canonical trading rules.

The uploaded reference GEX application is personal/non-commercial licensed. Its source is treated only as behavioral reference; project code is independently implemented and does not copy that implementation.

## High-speed update model

Each contract stores only the signed coefficient. Spot² is factored out because it is common to every contract. Therefore:

- underlying spot update: O(1) for net totals;
- existing contract gamma/OI update: hash lookup plus strike/expiration aggregate update;
- strike surface materialization: O(number of active strikes) only when a snapshot is requested.

A future Schwab adapter must merge partial stream messages into a complete contract state before calling `GexBook::upsert`.

## Data safety

The engine rejects:

- empty underlying / contract / expiration identity;
- underlying mismatches;
- non-finite or non-positive strike;
- non-finite or negative gamma;
- non-finite or non-positive multiplier;
- invalid timestamps;
- out-of-order spot or contract updates for the same identity;
- non-finite exposure results.

The snapshot exposes both oldest and newest contract quote timestamps. `as_of_millis` is conservative: the older of the spot timestamp and oldest contract timestamp. Provider-specific freshness durations remain outside this crate because canonical knowledge does not define them.

## Wall analytics

The analytics-only wall model selects:

- call wall: largest positive call exposure at a strike strictly above spot;
- put wall: largest absolute put exposure at a strike strictly below spot.

There is no fallback to open interest alone and no automatic reversal interpretation. Canonical §83 remains authoritative: walls are context and management references only.

## Gamma flip

Canonical §14 requires a gamma flip when reliable data are available but does not specify a calculation algorithm. The reference application uses its own heuristic; that heuristic is not promoted into this system as a canonical formula.

Until a gamma-flip model is separately researched, replay-tested, out-of-sample tested, shadow tested, and approved under §116, snapshots return:

```text
gamma_flip = None
gamma_flip_model = Unresolved
```

This does not block the rest of the GEX surface from being observed, but an unresolved flip cannot be presented as a reliable canonical gamma-flip input.
