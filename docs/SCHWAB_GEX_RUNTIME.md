# Schwab GEX runtime configuration

Status: market-data transport only. This layer cannot authorize or route a trade.

## Single profile

The existing environment form remains supported:

```text
SCHWAB_PROFILE_ID=default
SCHWAB_CLIENT_ID=<runtime-secret>
SCHWAB_CLIENT_SECRET=<runtime-secret>
SCHWAB_CALLBACK_URL=https://localhost/callback
SCHWAB_TOKEN_PATH=/secure/runtime/schwab-default.json
SCHWAB_REST_REQUESTS_PER_MINUTE=120
SCHWAB_REST_HEADROOM_REQUESTS_PER_MINUTE=<explicit-runtime-value>
SCHWAB_TRANSPORT_TIMEOUT_MS=<explicit-runtime-value>
SCHWAB_STREAM_RECONNECT_DELAY_MS=<explicit-runtime-value>
SCHWAB_GEX_STALE_TIMEOUT_MS=<explicit-runtime-value>
SCHWAB_MARKET_DATA_STALE_TIMEOUT_MS=<explicit-runtime-value>
SCHWAB_OPTION_CHAIN_REFRESH_INTERVAL_MS=<explicit-runtime-value>
```

Freshness, headroom, transport timeout, reconnect delay, and refresh values are not defaulted by the adapter because they are deployment/provider parameters rather than canonical strategy thresholds. Zero-valued timeouts and reconnect delays fail closed.

## Multiple API profiles

Set `SCHWAB_PROFILES_FILE` to a runtime-only JSON file. The software does not impose a maximum number of entries:

```json
{
  "profiles": [
    {
      "profile_id": "schwab-a",
      "client_id": "<runtime-secret>",
      "client_secret": "<runtime-secret>",
      "callback_url": "https://localhost/schwab-a/callback",
      "token_path": "/secure/runtime/schwab-a.json",
      "rest_requests_per_minute": 120,
      "rest_headroom_requests_per_minute": 20,
      "transport_timeout_ms": 5000
    },
    {
      "profile_id": "schwab-b",
      "client_id": "<runtime-secret>",
      "client_secret": "<runtime-secret>",
      "callback_url": "https://localhost/schwab-b/callback",
      "token_path": "/secure/runtime/schwab-b.json",
      "rest_requests_per_minute": 120,
      "rest_headroom_requests_per_minute": 20,
      "transport_timeout_ms": 5000
    }
  ]
}
```

The numeric values above are configuration examples only and are not strategy rules. Real client IDs, secrets, authorization codes, access tokens, and refresh tokens must never be committed. The reconnect delay remains global runtime configuration rather than a per-profile strategy value.

## Secret handling

Normal `Debug` formatting redacts client secrets and OAuth access/refresh tokens. Token files are validated before use. Token writes use a temporary file and atomic rename; on Unix the temporary token file is restricted to owner read/write permissions before rename. Runtime logging must still avoid manually printing exposed secret values.

## Connection scaling boundary

The adapter supports arbitrarily many configured API profiles and creates independent OAuth, REST-budget, token-store, and streaming identities for them. It does **not** assume that adding client IDs increases Schwab's allowed connections or request quota.

The current Schwab Streamer contract documents response code `12 CLOSE_CONNECTION` for reaching the connection maximum and states a limit of one Streamer connection at a time for a given user. For that reason, the adapter deduplicates simultaneous connection attempts by `schwabClientCustomerId` returned by User Preferences. Two API applications that resolve to the same Schwab streamer customer ID are not opened as parallel WebSockets.

Distinct streamer customer IDs can be connected concurrently by the software, but Schwab remains the final authority on provider-side application, account, entitlement, symbol, and connection limits. Response code `19 REACHED_SYMBOL_LIMIT` is handled as an explicit fail-closed provider limit rather than inventing a symbol-count threshold.

Current Issue #56 assignment is one underlying to one active profile. That allows multiple profiles to scale different underlyings while preserving one authoritative GEX state per underlying. Contract-level sharding of one very large underlying across multiple genuinely independent streamer identities is separate Issue #66 because it requires explicit cross-shard ownership, freshness, and coverage reconciliation.

## Data ownership

Each tracked underlying has one authoritative central GEX state. Underlyings are assigned across active profiles using deterministic sticky round-robin assignment. A profile's option and equity stream updates may mutate only the underlyings assigned to that profile.

A profile failure releases its assignments explicitly. The system does not silently duplicate one underlying across multiple GEX books or merge conflicting provider states.

## Transport timeout behavior

OAuth REST calls, market-data REST calls, WebSocket connection establishment, streamer login, WebSocket sends, and reads are bounded by the profile's explicit `transport_timeout_ms`. A timeout returns `TransportTimeout`; it never converts uncertain transport state into reliable GEX output.

The timeout value is operational configuration. The strategy knowledge does not define a universal timeout and the adapter therefore supplies no hidden default.

## Reconnect behavior

A WebSocket disconnect immediately makes the live streaming surface unavailable. The always-on profile supervisor waits the explicitly configured reconnect delay, obtains current streamer metadata, refreshes an expired or login-denied OAuth token when required, reconnects, and resubscribes the profile's existing underlying and option-symbol plan.

Reliable streaming status is restored only after every initial subscription acknowledgement succeeds. Change-only data received while acknowledgements are pending are buffered and replayed into the central state after activation. Provider connection/symbol-limit failures and provider-contract violations fail closed rather than entering an automatic retry loop.

Reconnect does **not** automatically launch an option-chain REST bootstrap. Coverage uncertainty or the explicit option-chain refresh schedule remains the authority for REST rebootstrap, preventing reconnect loops from becoming REST request storms.

## Gamma authority

The resulting GEX surface remains context only. Positive/negative gamma does not grant long/short direction, walls are not reversal signals, and unresolved gamma-flip logic remains unavailable rather than being guessed.
