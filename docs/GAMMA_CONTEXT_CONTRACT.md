# Gamma Context Contract

Governing canonical sections: §§14-15 and §25.

`GammaContext` is contextual information only. Positive gamma produces exactly the canonical dampened-volatility, higher-mean-reversion, and lower-breakout-persistence expectations. Negative gamma produces exactly amplified volatility and higher move acceleration. Dimensions the source does not specify for a regime remain absent rather than inferred.

`ReliableGammaSnapshot` represents a reliable GEX observation and requires nonempty underlying and timestamp metadata plus a Positive or Negative regime. Timestamp text remains opaque because canonical knowledge does not define a timezone, serialization format, or freshness duration. Supplied gamma flip/call wall/put wall values must be finite, but the values remain optional as allowed by §25.

`GammaRegime::Unavailable` remains the explicit no-reliable-GEX context and yields unknown/absent expectations. Gamma exposes no direction permission, order construction, risk override, or execution authority. Long/short permission remains governed by deterministic market state.
