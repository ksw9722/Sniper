# Passive retained-history Service Map (unreleased)

Service Map is a read-only view of metadata already retained by one Sniper
session. It builds the same inventory for the Scope UI and CLI. Opening or
refreshing it never probes an origin, starts a scan, replays a request, follows a
redirect, crawls a page, or asks an agent to explore a target. It does not create a
persistent graph or change the active session.

## Scope workspace

Scope keeps four views: Rules, Service Map, APIs, and Paths. Service Map uses a
stable card layout with origin search, scope filtering, viewport scrolling, and
an APIs/Evidence inspector. There is no force layout, drag editing, or pan/zoom
engine in this version. The renderer is separate from metadata normalization and
layout calculation so it can be replaced without changing the read contract.
The navigation strip follows the existing compact Capture-tab sizing. Long
origin, method, and path previews stay bounded; their full text remains available
in accessible text and hover titles, and selected endpoint evidence shows the
complete query-free path.

The graph displays at most 24 cards and 40 arrows. All origins keeps at most 100
additional reference-only candidate cards; those cards mean no captured rows are
shown in this snapshot, not that the origin has never been captured. Relationship and
source lists show at most 30 rows per pane, and API lists page through 50 exact
method/path entries at a time. These display caps are separate from server
projection limits, and the UI reports omitted or incomplete data explicitly.

Selecting a card or API entry only changes the local view. Viewing source reads
safe metadata from Sniper's exact-row evidence endpoint. Map and APIs never load
the legacy whole-history Paths response. Removal and retention events invalidate
displayed counts and evidence; other history changes label the previous snapshot
until the next bounded read completes. Exact method/path selection survives a
refresh only while that endpoint remains in the result; source verification is
cleared when its snapshot is invalidated. Local origin search keeps the inspector
and labels a selected origin outside the filter or card limit. Redirect
relationship rows include the query-free target path so different destinations
on the same origin remain distinguishable.

## Direct relations view

All origins remains the default graph view. Direct relations is an optional,
local view of the origins directly connected to a chosen anchor by loaded
Origin/Referer declarations or redirect instructions. It does not traverse
multiple hops or infer service calls. Selecting a different card changes the
inspector; an explicit Focus selection action changes the pinned anchor.
Returning to All origins keeps the inspector selection.

The anchor is included before the 24-card display cap. Local origin search
filters its neighbors while keeping the anchor visible; the view labels an
anchor outside the search. Counts distinguish all unique loaded neighbors,
query-hidden neighbors, card-cap omissions, and typed arrows among displayed
cards. Multiple captured redirect paths can share one typed origin-to-origin
arrow; their exact paths remain separate in the inspector. The compact summary
keeps nonzero omissions visible; Relation details exposes complete typed-link
counts, direction definitions, and display limits.

Neighborhood discovery uses the full bounded metadata response, independently
of the All origins view's reference-card cap. It cannot recover metadata omitted
by server projection limits or retention. A reference-only anchor remains a
metadata reference, with no observed traffic count. No loaded neighbors means
only that this snapshot has no directly related other origins for the anchor.

Mode changes, focusing, and card selection send no requests. Scope filtering
still reads fresh retained metadata. Temporary invalidation clears displayed
data, and a fresh result that no longer contains the anchor returns to All
origins. A session change resets the mode and evidence immediately.

## Read through the CLI

Discover the contract offline before reading a running local instance:

```bash
sniper-cli schema input target.service_map.get
sniper-cli schema output target.service_map.get
sniper-cli examples target.service_map.get
sniper-cli scope service-map --session-id <session-uuid>
sniper-cli scope service-map --session-id <session-uuid> --in-scope-only --max-records 10000
sniper-cli call target.service_map.get --input '{"session_id":"00000000-0000-0000-0000-000000000000","in_scope_only":false,"max_records":10000}'
```

The UUID in the JSON example is synthetic. Select a real session deliberately
from session discovery. `target` remains an alias for the `scope` command group.
All three Service Map operations have `side_effect: "read"` and need no `--yes`.
`--dry-run` describes the local API request without contacting it.

`session_id` is optional for the map: omission or null resolves the active session
once, then pins that UUID for the read. An explicit UUID can select an inactive
saved session without switching the desktop. Reuse the returned `session_id` for
evidence reads. Direct commands return the bare result; `call` returns the same
result in `data`, with the pinned session in `meta.session_id`.

`max_records` defaults to 10,000, accepts integers from 1 through 50,000, and bounds
retained metadata rows considered. Optional `--origin https://example.com` (or
`origin` in `call`) selects one exact canonical HTTP(S) origin. Supply no path,
query, fragment, credentials, or explicit default port; copy the canonical origin
from a map card. `in_scope_only` defaults to false and uses the
same session scope matcher as other Sniper views. These are read filters, not
capture or scope-setting controls. There is no pagination cursor, URL input,
active-discovery option, or automatic retry with a larger limit.

## What the map means

- An origin is an observed scheme, host, and effective port. It is not a discovered
  server, IP address, ownership claim, or infrastructure boundary.
  Scope is evaluated against each original captured authority, as in History.
  If canonical aliases merge into one origin card, `in_scope` means at least one
  analyzed row matched; `in_scope_only` filters individual rows before counting.
- Endpoints preserve the observed method and exact path. Query strings and URL
  credentials are omitted. Numeric and UUID path-segment grouping is only a
  conservative inventory hint; it does not prove a route template or merge
  distinct exact endpoints into one resource. Exact paths can still contain
  application-provided identifiers; keep exported map output private.
- Request counts count retained rows, including rows with duplicate transaction
  UUIDs. They are not unique users, unique requests over all time, or complete
  production traffic totals.
- Status and content-type summaries describe captured metadata. Missing response
  data is not a successful response, and absence of an observation is not evidence
  that a service or endpoint does not exist.
- A redirect relationship means a captured redirect response instructed a client
  to use a Location. It does not establish that any client followed it, that the
  target was contacted, or that the two origins call each other.
- Origin and Referer links are request-header declarations. They do not prove
  which page, browser process, or service initiated a request. Header-only origins
  do not create observed-service cards.
- Technology clues are limited observations of response headers. They are not
  verified product/version detection and do not imply an application, proxy, CDN,
  or database chain. Raw header values, including cookie and authorization values,
  and request or response bodies are not included in the map or its safe evidence
  view.

The result lists initiator, remote IP, and TLS certificate in `unavailable`.
The default map continues to list response-body references as `deferred`: bodies
are never read to build or refresh the map or its metadata evidence. A separate,
explicit one-row retained-preview read is described below. It does not enrich the
default inventory automatically. Do not fill unavailable gaps using hostname
guesses or present them as collected evidence.

## Completeness and freshness

The bounded window uses newest capture-sequence order, not wall-clock time.
`first_retained_started_at` and `last_retained_started_at` are timestamps at the
retained sequence boundaries, not an all-history temporal minimum and maximum.
`oldest_considered_at` and `latest_considered_at` describe the considered window.
CONNECT tunnel rows are counted separately in `non_http_rows` and do not become
HTTP endpoint observations.

Read `snapshot.complete`, `snapshot.stopped_by`, the reported row counts, limits,
and warnings before interpreting the inventory. Results are bounded and describe
only the retained rows represented in that result. Retention, deletion, scope
filtering, invalid metadata, and per-result limits can leave data out. A complete
retained-history result is still not a complete network architecture or a record
of traffic that was never captured or has already been removed.

Refreshing computes a new view from current metadata. Counts and scope membership
can change. An evidence token is a locator for a particular row state, not a
promise that its underlying record will remain available indefinitely. Reopening
an inactive session can change its store generation and invalidate an earlier
token. Snapshot IDs are not persistent graph IDs or pagination cursors.

## Exact-row evidence

```bash
sniper-cli schema output target.service_map.evidence.get
sniper-cli scope service-map-evidence --session-id <returned-session-uuid> --evidence-id <returned-evidence-token>
sniper-cli call target.service_map.evidence.get --input '{"session_id":"00000000-0000-0000-0000-000000000000","evidence_id":"<returned-evidence-token>"}'
```

Both fields are required. The token is at most 200 UTF-8 bytes; copy it unchanged from a map evidence
array. It binds the session's retained-store generation, row sequence, transaction
UUID, and row revision. A transaction UUID alone is insufficient when retained
rows share an ID.

Check `available` before using `row`. A stale, edited, removed, or otherwise
unresolvable row returns explicit unavailability rather than silently substituting
another record. The evidence read returns only safe metadata for the exact row.
It does not hydrate bodies, expose raw headers, or call the usual UUID-only HTTP
detail endpoint as a fallback. Refresh the map deliberately if you need current
evidence; do not describe unavailable historical evidence as verified.

## Explicit one-row response-body references

This optional read requires a deliberate selection of one exact retained row:

```bash
sniper-cli schema input target.service_map.body_references.get
sniper-cli schema output target.service_map.body_references.get
sniper-cli examples target.service_map.body_references.get
sniper-cli target service-map body-references --session-id <active-session-uuid> --evidence-id <returned-evidence-token>
sniper-cli call target.service_map.body_references.get --input '{"session_id":"00000000-0000-0000-0000-000000000000","evidence_id":"<returned-evidence-token>"}'
```

Both fields are required. Copy the complete evidence token unchanged. The chosen
session must already be active: an inactive session returns `unavailable` with
`inactive_session`, without loading or activating it. No session lookup, switch,
UUID-only detail fallback, body batch, or automatic retry occurs. Session and
row changes during a read invalidate its result. Tokens obtained for an inactive
metadata inventory may become stale if that session is later opened; deliberately
read a fresh map before inspecting one of its rows.

The Scope inspector offers an explicit action for the selected evidence row.
Selecting or expanding an origin, API, relationship, or evidence item does not
read bodies. Body-reference results stay separate from observed traffic and
header-derived links. Closing or changing the selection/session invalidates a
pending inspector response.

The reader inspects only the retained response preview of UTF-8 HTML or
JavaScript. Supported media types, ignoring case and parameters, are `text/html`,
`application/xhtml+xml`, `application/javascript`, `text/javascript`,
`application/ecmascript`, `text/ecmascript`, and `application/x-javascript`.
An encoded body is accepted only if capture already decoded it, or all declared
content encodings are `identity`. The read does not decompress content, decode
Base64 or entities/JavaScript escapes, parse a document, execute JavaScript,
resolve relative URLs, or contact any target. Missing, unsupported, stale, and
unverifiable source data are explicit states rather than empty successful scans.

`coverage: "literal_absolute_http_urls"` means bounded literal absolute HTTP(S)
URL references. Scheme matching is ASCII case-insensitive; a start immediately
after an ASCII letter, digit, or underscore is ignored. Candidates end at ASCII
whitespace/control characters, quotes, angle brackets, backticks, backslashes,
parentheses, or braces. The reader parses each bounded candidate as an absolute
HTTP(S) URL, then retains only its canonical origin. A final candidate cut by
capture or prefix truncation is omitted conservatively. It cannot establish
runtime calls, followed links, initiators, ownership, or service dependencies. An
empty complete result means no references were found within this supported
retained-preview grammar, not that the response
has no URLs or that the application makes no other calls.

The response has `schema_version: 1`, the selected `session_id` and `evidence_id`,
`available`, `status`, `complete`, and `reasons`. Status is one of:

- `scanned`: the supported retained preview was inspected within the limits;
  `available: true`, `complete: true`, and no reasons
- `partial`: a capture, prefix, candidate, origin, or offset limit left coverage
  incomplete; `available: true`, `complete: false`, with reasons
- `unsupported`: the retained representation cannot be inspected by this reader;
  `available: false`, `complete: false`
- `unavailable`: the exact retained row or required data cannot be read safely;
  `available: false`, `complete: false`

Available responses identify `source_origin`, decimal-string `source_sequence`,
`source_started_at`, and `retained_preview_bytes`. Source metadata may be omitted
when unavailable. `references` contains only `target_origin`, `occurrences`,
`preview_byte_offsets`, and `omitted_offsets`. Origins are canonical scheme/host/
port values; response text, snippets, paths, queries, fragments, and URL credentials
are never returned. Origin names can still reveal private service information.

Offsets are zero-based UTF-8 byte offsets in the retained preview,
`offset_basis: "retained_utf8_preview"`. They are not offsets in the full body,
compressed response, or wire traffic. Capture truncation and scan-prefix truncation
are reported separately as `capture_preview_truncated` and
`scan_prefix_truncated`; inspect both along with `scanned_bytes`.

The fixed `limits` are:

- `scan_bytes`: 262144 retained UTF-8 preview bytes, ending on a character boundary
- `serialized_row_bytes`: 2097152 bytes for one disk-backed serialized row
- `origins`: 100 distinct referenced origins
- `candidates`: 4096 candidates, counted in `candidates_scanned`
- `candidate_bytes`: 4096 bytes per literal URL candidate
- `offsets_per_origin`: 5 stored offsets per referenced origin

Resident preview length may exceed the disk-row limit; only its bounded prefix is
inspected. An overlarge disk row is refused before hydration. Origin-cap omissions
are counted in `omitted_reference_occurrences` only for validated references;
further candidates beyond the candidate cap have unknown reference coverage.
`omitted_offsets` counts additional occurrences whose offsets were not retained.
Reasons name each applicable cap or unsupported/unavailable state. No automatic
larger read follows a partial result. Even `complete: true` describes only this
supported literal grammar within one retained preview, never all runtime calls.

## Local API

- `GET /api/target/service-map?session_id=<uuid>&in_scope_only=false&max_records=10000`
- `GET /api/target/service-map/evidence?session_id=<uuid>&evidence_id=<opaque-token>`
- `GET /api/target/service-map/body-references?session_id=<active-uuid>&evidence_id=<opaque-token>`

URL-encode the opaque token as a query parameter. These endpoints use the existing
local API access control. Service Map discovery and reads do not follow API
redirects or retry failed requests. Discovery validates the usual Sniper identity;
when it reads local runtime-state, it preserves that file even if it is stale or
invalid and does not create lock or recovery files. A CLI schema describes the installed CLI; an older
running Sniper may not provide the Service Map endpoints. Never fall back to
active traffic generation when a read is unsupported or incomplete.
