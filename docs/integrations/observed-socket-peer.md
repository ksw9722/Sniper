# Observed socket peer capture (unreleased)

Observed socket peer capture is an explicit, per-session option, **Off by
default**. It records at most one bounded socket observation for an eligible
future ordinary captured HTTP(S) response. Opening Service Map, its source
inspector, or settings never enables it. This feature sends no probes and does
not resolve DNS, inspect TLS, infer an origin IP, or build an aggregate IP map.

## What an observation means

The observation reports the remote address supplied by the HTTP client for the
response that actually arrived, plus the route of that successful attempt:
`direct`, `http_proxy`, `socks_proxy`, or `unknown`. It is taken when upstream
response headers arrive, before local response edits. HTTP error statuses still
qualify as actual responses; connection failures and locally generated responses
do not. If the client does not expose a remote address, an eligible response can
record `remote_addr_unavailable` instead of an address.

Eligibility is frozen at the ordinary capture caller before its first send.
Turning the option On later does not collect a request that started ineligible.
An eligible in-flight request can still finish with an observation after the
option is turned Off. Route classification describes the successful attempt,
including an actual proxy fallback; it is not inferred from current settings
when the result is viewed. A reused connection is not evidence of a new connection
or TLS handshake.

A socket peer can be a proxy, CDN, shared endpoint, or other intermediary. It
**does not establish origin ownership, a backend chain, or a service dependency**.
A proxy route is not an origin-IP observation. No hostname, proxy credentials,
TLS certificate, DNS answers, or body content is added to the observation.
Replay, WebSocket, CONNECT, rendered-resource fetches, synthetic/local responses,
and historical backfill are excluded.

## Capture Settings

The separate **Upstream socket peer** card in Capture Settings has an Off-by-default
switch and explicit Save/Cancel controls. Changing the local draft does not save
it. Cancel discards the draft. Leaving and reopening settings preserves a
same-session draft while checking the latest saved setting; changing session
or workspace generation invalidates it. The card shows the saved switch separately from `effective_enabled` and any block. Capture can remain effectively
Off while HTTP capture is paused, configuration is invalid, or persistence is
uncertain.

A save is session-bound and compare-and-swap protected. After an uncertain save,
read settings for that same active session before deciding whether to save again.
Do not retry automatically, navigate to another session to reconcile it, or treat
a lost acknowledgement as proof that nothing changed. Inactive-session requests
are rejected without loading or activating that session. The persistence-uncertain
capture block is an in-memory safety gate; after a restart, a valid saved switch
is used. A successful explicit save can confirm the candidate durable state;
merely reading settings does not clear the gate.

## CLI

Both commands require an explicit current active session UUID:

```bash
sniper-cli capture http upstream-peer get --session-id <active-session-uuid>
sniper-cli capture http upstream-peer set --session-id <active-session-uuid> --enabled true --dry-run
sniper-cli capture http upstream-peer set --session-id <active-session-uuid> --enabled true --yes
sniper-cli capture http upstream-peer set --session-id <active-session-uuid> --enabled false --yes
sniper-cli schema input capture.http.upstream_peer.set
sniper-cli schema output capture.http.upstream_peer.get
sniper-cli examples capture.http.upstream_peer.set
sniper-cli call capture.http.upstream_peer.set --input '{"session_id":"00000000-0000-0000-0000-000000000000","enabled":false}' --dry-run
```

The JSON UUID is synthetic. Select the current active UUID deliberately.
Canonical operations are `capture.http.upstream_peer.get` and
`capture.http.upstream_peer.set`. Input accepts only `session_id` and, for a set,
a required JSON boolean `enabled`. Nulls, strings such as `"true"`, token
injection, batch settings, and backfill options are rejected. A set requires
`--yes`; dry-run, schema, manifest, and examples are offline.

Execution reads the selected session's narrow settings once, then sends at most
one token-bound POST. Both requests carry the same `session_id` and
`expected_active_session_id`; the command never switches sessions. There is no
redirect following, automatic POST retry, or uncertain-write readback. Returned
metadata is validated and projected to a fixed allowlist. Direct commands return
the result; canonical `call` returns it in `data` with `meta.session_id`.

## Narrow settings API

- GET `/api/transactions/upstream-peer-settings?session_id=UUID&expected_active_session_id=UUID`
- POST the same path with `{"enabled":true,"expected_config_token":"<unchanged GET token>"}`

The response contains:

- `session_id`: selected UUID
- `enabled`: saved per-session switch
- `config_revision`: canonical decimal unsigned 64-bit string, preserving large revisions exactly
- `config_token`: opaque 64-character lowercase hexadecimal token bound to this feature, session, configuration, and revision
- `status.effective_enabled`: whether new eligible work can currently record observations
- `status.blocked_reason`: null, `persistence_uncertain`, `invalid_configuration`, or `http_capture_paused`
- `changed`: POST only, whether the saved switch changed

A changed switch increments its revision; a no-op preserves it. Invalid stored
revision state fails closed and uses the maximum decimal value in the response;
its opaque token remains distinct from a valid maximum revision. Do not derive
or reuse tokens across features. Missing tokens, malformed inputs, stale tokens,
inactive sessions, or active-session fence conflicts do not authorize a write.
An acknowledgement means the settings operation returned successfully; it does
not prove that an observation exists or that a particular request was eligible.

## Explicit exact-row evidence

The default Service Map and default evidence shapes remain unchanged. To inspect
one retained observation, use the exact session and evidence token from the map:

```bash
sniper-cli target service-map-evidence --session-id <returned-session-uuid> --evidence-id <returned-token> --include-socket-peer
sniper-cli call target.service_map.evidence.get --input '{"session_id":"00000000-0000-0000-0000-000000000000","evidence_id":"<returned-token>","include_socket_peer":true}'
```

Only explicit true adds `include_socket_peer=true` to the GET query. False or
omission sends no peer query parameter. The optional `socket_peer` envelope is
returned only after successful exact-token validation (`available: true`):

- `observed`: `observation` has a typed IPv4/IPv6 `address.ip`, unsigned 16-bit `address.port`, and `reason: null`
- `unavailable`: an eligible response's observation has `address: null` and `reason: "remote_addr_unavailable"`
- `not_recorded`: `observation: null`; no recorded socket peer is available for this row

A non-null observation also contains `source: "http_response_remote_addr"`,
`observed_at` (response-header timestamp), and `route`. Each observation is
bounded to 512 serialized bytes. An old row, an ineligible row, or a dropped
invalid extension can have no observation. `not_recorded` does not tell you why
and must not be presented as proof of a direct route or failed connection.

Stale, edited, deleted, cross-session, and ambiguous duplicate-ID evidence fails
closed with the ordinary evidence unavailability response and no peer envelope.
There is no UUID-only detail fallback, automatic capture activation, aggregation,
IP edge, origin-IP claim, or relationship traversal. A setting being On now does
not establish that an older row was eligible.

## Retention and disclosure

Observations are retained in session files and included in session exports and
full-record JSON. Treat socket addresses as potentially sensitive infrastructure
metadata before sharing those files. Turning the switch Off stops eligibility
for new work; it does not erase previously retained observations. There is no
separate erasure, retroactive capture, or backfill control in this feature.

Peer fields are absent from transaction summaries, summary event streams, and
the default aggregate Service Map. Existing read-only discovery and map reads
do not start capture, activate workers, inspect TLS, or contact mapped services.
