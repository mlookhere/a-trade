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
SCHWAB_GEX_STALE_TIMEOUT_MS=<explicit-runtime-value>
SCHWAB_MARKET_DATA_STALE_TIMEOUT_MS=<explicit-runtime-value>
SCHWAB_OPTION_CHAIN_REFRESH_INTERVAL_MS=<explicit-runtime-value>
```

No freshness, headroom, or refresh values are defaulted by the adapter because they are deployment/provider parameters rather than canonical strategy thresholds.

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
      "rest_headroom_requests_per_minute": 20
    },
    {
      "profile_id": "schwab-b",
      "client_id": "<runtime-secret>",
      "client_secret": "<runtime-secret>",
      "callback_url": "https://localhost/schwab-b/callback",
      "token_path": "/secure/runtime/schwab-b.json",
      "rest_requests_per_minute": 120,
      "rest_headroom_requests_per_minute": 20
    }
  ]
}
```

The example values are configuration examples only and are not strategy rules. Real client IDs, secrets, authorization codes, access tokens, and refresh tokens must never be committed.

## Connection scaling boundary

The adapter supports arbitrarily many configured API profiles and creates independent OAuth, REST-budget, token-store, and streaming identities for them. It does **not** assume that adding client IDs increases Schwab's allowed connections or request quota.

The current Schwab Streamer contract documents response code `12 CLOSE_CONNECTION` for reaching the connection maximum and states a limit of one Streamer connection at a time for a given user. For that reason, the adapter deduplicates simultaneous connection attempts by `schwabClientCustomerId` returned by User Preferences. Two API applications that resolve to the same Schwab streamer customer ID are not opened as parallel WebSockets.

Distinct streamer customer IDs can be connected concurrently by the software, but Schwab remains the final authority on provider-side application, account, entitlement, symbol, and connection limits. Response code `19 REACHED_SYMBOL_LIMIT` is also handled as an explicit fail-closed provider limit rather than inventing a symbol-count threshold.

## Data ownership

Each tracked underlying has one authoritative central GEX state. Underlyings are assigned across active profiles using deterministic sticky round-robin assignment. A profile's option and equity stream updates may mutate only the underlyings assigned to that profile.

A profile failure releases its assignments explicitly. The system does not silently duplicate one underlying across multiple GEX books or merge conflicting provider states.

## Reconnect behavior

A WebSocket disconnect immediately makes the live streaming surface unavailable. It does **not** automatically launch an option-chain REST bootstrap. The existing option universe is resubscribed after a successful reconnect; REST rebootstrap occurs only when coverage becomes uncertain or the explicitly configured refresh schedule requires it. This prevents reconnect loops from becoming REST request storms.

## Gamma authority

The resulting GEX surface remains context only. Positive/negative gamma does not grant long/short direction, walls are not reversal signals, and unresolved gamma-flip logic remains unavailable rather than being guessed.
