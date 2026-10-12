# Passive retained-history Service Map (unreleased)

Service Map's default inventory is a read-only view of metadata already retained
by one Sniper session. It builds the same inventory for the Scope UI and CLI.
Opening or refreshing it never probes an origin, starts a scan, replays a request,
follows a redirect, crawls a page, or asks an agent to explore a target. It does not
create a persistent graph or change the active session.

## Scope workspace

Scope keeps four views: Rules, Service Map, APIs, and Paths. Service Map uses a
stable card layout with origin search, scope filtering, viewport scrolling, and
an APIs/Evidence inspector. Compact 220×104px cards at the default 12px Display
text size share their geometry with all arrow attachments. Larger Display text
grows card height and row spacing together (146px cards at 20px), retaining
legible line boxes, aligned arrows, selection, and the chosen zoom. Fit, minus, the percentage/reset button, and plus scale
only the graph from 10% to 200%; the inspector and Scope navigation retain their
normal size. Fit is a one-shot action for the displayed cards, bounded by those
scale limits. Polling and resizing never refit or change the chosen zoom. Plus
and minus preserve the viewport center where scroll bounds allow. With the map
viewport focused, +/−, 0, and F provide zoom, reset, and Fit shortcuts. Zooming is
local and preserves selection, exact-source reads, and temporary body overlays.
There is no force layout or drag editing in this version. The renderer is separate from metadata normalization and
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
An explicit temporary body-reference overlay can add references from one
already-inspected response, as described below. Returning to All origins clears
that overlay and keeps the inspector selection only if it still belongs to the
metadata inventory.

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
All four Service Map operations have `side_effect: "read"` and need no `--yes`.
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

## Reported header products and versions

New servers add optional `technology_observations` arrays beside the unchanged
legacy `technology_clues`, both on observed origins and exact evidence rows.
An absent extension means the server does not supply this contract; an empty
array means no supported literal declaration was retained in that projection.
The map's `schema_version` remains 1. Existing clients may ignore the extension;
new clients must continue to accept older responses without it.

These observations read only response headers already retained with the row.
They do not read a body, send a request, enable collection, change scope or
settings, resolve DNS, infer an address or TLS identity, or assess vulnerabilities.
Headers can be spoofed, stale, suppressed, or supplied by an intermediary.
Treat a numeric version as a **reported version**, never a verified installation
or an authoritative current version. Catalog categories such as `web_server`,
`application_server`, `language_runtime`, `edge_service`, `cms`, `connector`, and
`unknown` are product labels, not observed physical WEB/WAS or backend roles.

The frozen catalog revision 1 header/product allowlist is:

- `Server`: nginx, Apache, Caddy, Cloudflare, Microsoft-IIS, Envoy, PHP, Jetty,
  Apache-Coyote, and the literal two-word declaration Apache Tomcat
- `X-Powered-By`: PHP, Express, and ASP.NET, at the beginning of the value
- `X-Generator`: WordPress and Drupal, at the beginning of the value

Catalog revision 2 includes revision 1 and exactly these additions:

| Header | Literal | Product ID | Name | Category | Rule ID |
| --- | --- | --- | --- | --- | --- |
| `Server` | `gunicorn` | `gunicorn` | Gunicorn | `application_server` | `server.gunicorn.v1` |
| `Server` | `uvicorn` | `uvicorn` | Uvicorn | `application_server` | `server.uvicorn.v1` |
| `Server` | `waitress` | `waitress` | Waitress | `application_server` | `server.waitress.v1` |
| `Server` | `openresty` | `openresty` | OpenResty | `web_server` | `server.openresty.v1` |
| `Server` | `LiteSpeed` | `litespeed` | LiteSpeed | `web_server` | `server.litespeed.v1` |
| `X-Powered-By` | `Next.js` | `next_js` | Next.js | `application_framework` | `x_powered_by.next_js.v1` |

These additions follow primary-source header declarations in
[Gunicorn](https://github.com/benoitc/gunicorn/blob/master/gunicorn/http/wsgi.py),
[Uvicorn](https://github.com/Kludex/uvicorn/blob/main/uvicorn/config.py),
[Waitress](https://docs.pylonsproject.org/projects/waitress/en/latest/arguments.html),
[OpenResty](https://openresty.org/en/ann-1027001002.html),
[LiteSpeed](https://docs.litespeedtech.com/lscache/tips/), and
[Next.js](https://github.com/vercel/next.js/blob/canary/packages/next/src/server/send-payload.ts).
They use only the existing bare-product and slash-version grammar. A bare product
has `not_reported` version status; an unsupported suffix keeps the product but
returns a null version, never a numeric prefix. No edition, installed version,
companion product, or infrastructure role is inferred. In particular, OpenResty
adds no nginx or Lua observation, LiteSpeed adds no edition or OpenLiteSpeed
observation, and Next.js adds no React or Node.js observation. Rule suffixes
remain `.v1`: catalog revisions and individual rule revisions are independent.

Matching is ASCII case-insensitive, with complete product-name boundaries.
`Server` can supply several whitespace-separated products. Nested and escaped
HTTP comments are skipped, including product-like words inside them. A product
concatenated directly after a comment or `Jetty(...)` is not promoted into an
additional observation. Arbitrary header names, unknown products, hostnames,
body strings, and product-name substrings are not evidence. The legacy clues
still use their original leading-product extraction; a new observation need
not have a matching legacy clue.

Versions use `/` after the product, `Jetty(version)`, or whitespace after a
supported `X-Generator` product. Only a complete nonempty ASCII numeric token
or dot-separated numeric components is accepted, at most 32 UTF-8 bytes.
Suffixes such as `1.2-custom`, `9.4.51.v20230217`, arbitrary strings, empty
components, and overlong values never become a shortened numeric version.
The explicit `version_status` is:

- `reported`: `reported_version` contains the complete accepted numeric value
- `not_reported`: the supported literal declaration has no version separator
- `unsupported_format`: a declared version does not fit the supported grammar;
  `reported_version` is null, and no rejected text is returned
- `non_product_version`: a numeric Apache-Coyote connector declaration is not an
  installed product version; `reported_version` is null

In particular, `Apache-Coyote/1.1` means an Apache-Coyote connector declaration.
It never becomes Tomcat 1.1. Only a literal Apache Tomcat declaration produces
the `apache_tomcat` catalog entry. Unsupported Jetty release suffixes are explicit
unsupported formats, not silently reduced to a numeric prefix.

Each exact-row observation includes stable `product_id`, `name`,
`catalog_category`, nullable `reported_version`, `version_status`, fixed
`source: "response_header"` and `strength: "literal_declaration"`, lowercase
`header_name`, and a fixed allowlisted `rule_id`. Its zero-based `header_index`
addresses the original retained response-header array, including unrelated and
duplicate headers. `product_span` and nullable `version_span` have integer
`start` and exclusive `end` positions with
`offset_basis: "retained_header_value_utf8"`. These are byte offsets into the
original retained value, including leading whitespace and Unicode comments,
not character positions, normalized values, or original wire bytes. Only
accepted reported versions have a version span. Raw headers, comments, snippets,
unrecognized tokens, and rejected version strings are never returned.

Origin observations aggregate by product, header name, reported version, and
version status. Each has `row_count`, `first_seen`, `last_seen`, and up to three
exact evidence tokens. Repeated identical declarations within one row count
once; separate retained rows count separately even when transaction UUIDs repeat.
The timestamps are the minimum and maximum start times of contributing rows.
Conflicting versions remain separate entries. Sorting does not choose a highest,
newest, or authoritative version. Use an entry's exact evidence token to inspect
its retained source row under the normal freshness checks below.

The optional limit fields are `technology_observations_per_row: 8`,
`technology_observations_per_origin: 32`, `technology_observations: 2000`, and
`technology_version_bytes: 32`, plus `technology_token_candidates_per_row: 128`.
The token-work budget is shared across the eligible response headers in one row.
Product candidates, unknown or malformed tokens, and comments all consume it;
whitespace does not. One complete product declaration includes its attached
version, and a nested HTTP comment is one comment candidate. An unseparated
malformed suffix after a comment or `Jetty(...)` is a separate candidate to skip,
never a new product. Candidates are not shortened to fit a remaining budget.
Exactly 128 inspected candidates at the end is complete; a further uninspected
candidate adds `technology_token_scan_limit` to row `omitted` and map
`snapshot.stopped_by`. A partial row can therefore contain zero observations.
This work cap does not stop the existing legacy clue or redirect-header scan.
The existing combined 128-header, 32-KiB
header-byte, 4-KiB header-value, and 8-MiB metadata-projection budgets still apply.
Excess row observations add `technology_observations_limit` to row `omitted`
and map `snapshot.stopped_by`. Origin/map tuple caps report
`technology_observations_per_origin_limit` or `technology_observations_map_limit`
in `stopped_by`. Counts and times for retained tuples continue updating after
aggregate caps; other map metadata continues projecting. Counts still cover
only bounded projected observations, so row/token/header/window omissions must remain
visible. No automatic larger scan follows a partial result.

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

## Explicit one-row observed socket peer

The default inventory and evidence response do not include socket peer data.
An explicit `--include-socket-peer` on `scope service-map-evidence`, or boolean
`include_socket_peer: true` on `target.service_map.evidence.get`, adds one optional
`socket_peer` envelope only after exact-row token validation succeeds. The
statuses are `observed`, `not_recorded`, and `unavailable`; check the status and
nullable observation before using an address. False or omission sends no peer
query parameter. No peer is returned for stale or unavailable evidence.

Capture is independently Off by default and requires an explicit active-session
settings save. This read never enables it, backfills rows, probes, or activates a
session. A socket peer can be a proxy, CDN or shared endpoint, not proof of origin
ownership or a backend chain. No aggregate peer map or IP edges are added. Retained
observations are included in session files, exports and full-record JSON; Off
does not erase them. See [the complete capture and evidence contract](observed-socket-peer.md).

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
pending inspector response. A completed result does not change the graph unless
the operator separately chooses **Show references on map**.

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

## Explicit retained HTML generator declarations

**Inspect retained HTML generators** is a separate explicit inspection of one
verified exact response in the active session. Selecting evidence, opening or
polling the map, refreshing metadata, zooming, and opening a disclosure never
read response bodies. This action does not enrich map cards, header observations,
the API inventory, or the URL-reference overlay. Inspecting URL references and
HTML generators requires two separate explicit bounded reads.

```bash
sniper-cli schema input target.service_map.html_generators.get
sniper-cli schema output target.service_map.html_generators.get
sniper-cli scope service-map html-generators --session-id <active-session-uuid> --evidence-id <opaque-token>
sniper-cli call target.service_map.html_generators.get --input '{"session_id":"00000000-0000-0000-0000-000000000000","evidence_id":"<opaque-token>"}'
```

Both selection fields are required. The `target` command alias also works.
This read operation needs no `--yes`; `--dry-run` makes no request. An older
server without the endpoint is unsupported; there is no fallback fetch or retry.
The generator catalog is independently fixed at revision 1 and does not use the
header-technology catalog negotiation header.

Only already-retained UTF-8 `text/html` previews are eligible. XHTML, JavaScript,
base64/binary previews, and undecoded non-identity content encodings are refused.
No response is fetched, decoded, replayed, evaluated, or persisted by this action.
The reader copies only bounded metadata and a bounded resident prefix under the
store lock. Lazy rows use a bounded serialized-row read outside that lock,
without hydrating the full transaction. Sequence, UUID, row revision, file
locator/generation, and active-session identity are rechecked before results can
be published. An inactive session is rejected without loading its saved context
or starting its worker. Retention, deletion, mutation, or source replacement
invalidates the result. Existing body-reference behavior and output are unchanged.

This is a deliberately incomplete literal recognizer, not a general HTML parser.
It requires explicit `html` and `head` elements and quoted literal `name` and
`content` values on `meta` declarations. Matching is ASCII case-insensitive.
Only complete WordPress and Drupal product names are returned, as `wordpress` /
`WordPress` or `drupal` / `Drupal`, with `catalog_category: "cms"`. A whole
reported version may follow 1–32 original HTML ASCII whitespace bytes. The
version must contain only nonempty dot-separated ASCII numeric components and
be at most 32 bytes. Unsupported suffixes retain the recognized product with
`version_status: "unsupported_format"`, `reported_version: null`, and
`version_span: null`; no numeric prefix or rejected text is returned. Bare
products have `not_reported`; accepted versions have `reported`.

Unrelated quoted attributes and Boolean attributes remain opaque. Supported
comments and literal title/style/script raw text are skipped safely, without
searching their text for generator tags. There is no entity decoding or HTML
error recovery. Entities in semantic generator fields, malformed tags, duplicate
attributes, unsupported script escapes, templates, foreign content, and ambiguous
contexts stop partial inspection. Unknown products are counted only among
actually inspected quoted generator declarations; their text is never returned.

### Generator result contract

Every result includes `schema_version: 1`, `generator_catalog_version: 1`, the
requested `session_id` and `evidence_id`, `available`, `status`, `complete`, and
fixed-code `reasons`. `status` is `scanned`, `unsupported`, or `unavailable`.
Accepted partial inspections remain `available: true`, `status: "scanned"`,
`complete: false`. Unsupported and unavailable results return no observations,
clear counters/flags, and explicitly null source metadata. Never interpret a
partial or unsupported result as “generator not found.”

Available results contain `source_origin`, decimal-string `source_sequence`,
`source_started_at`, and `retained_preview_bytes`. `scan_prefix_bytes` is the
bounded prefix offered to the recognizer, **not** a claim every byte was parsed.
`capture_preview_truncated` and `scan_prefix_truncated` are independent. When a
bounded retained prefix ends while parsing, the adapter reports `input_limit`;
when only capture is truncated, its stop remains `end_of_preview`, alongside
the capture-truncation reason. `head_closed` means the supported explicit head
closed. It may remain true with `parser_stop: null` when an unparsed trailing
body was truncated; `complete` is still false for any capture or prefix limit.
Parser, tag, attribute, candidate, or observation limits also make it false.
Even true completeness covers only this supported head grammar, never all
possible generators or runtime products.

`coverage` is always `literal_html_generator_explicit_head`. A stopped parser
returns `parser_stop: {reason, byte_cursor}`. The cursor is a raw-byte position
that may fall inside a UTF-8 character at a byte cap and must never be used to
slice text. Accepted `tag_span`, `content_span`, `product_span`, and nullable
`version_span` are half-open, character-boundary-safe byte ranges in the original
retained UTF-8 preview. Both envelope and observations use
`offset_basis: "retained_utf8_preview"`; these are not wire-body offsets.

Each observation has the product/name/category, nullable `reported_version`,
`version_status`, `source: "response_html_meta_generator"`,
`strength: "literal_declaration"`, spans, offset basis, and a fixed rule ID:
`html_meta_generator.wordpress.v1` or `html_meta_generator.drupal.v1`.
There are no raw tags, body snippets, unknown product names, or rejected version
strings. Declarations can be spoofed or stale; they do not establish an installed
version, backend stack, infrastructure role, ownership, or vulnerability.

`tag_candidates_inspected` counts encountered start/end tags and doctype, not
comments or raw-text decoys. `additional_tag_candidate_encountered` becomes true
only when an actual next candidate exceeds the cap.
`additional_observation_encountered` becomes true only for an actual ninth
recognized supported declaration. No uninspected omission count is guessed.
`unknown_product_declarations` is an exact count within the inspected prefix only.

The fixed `limits` object contains:

- `scan_bytes`: 262144 retained UTF-8 bytes, ending on a character boundary
- `serialized_row_bytes`: 2097152 bytes for one disk-backed serialized row
- `tag_candidates`: 4096
- `tag_bytes`: 4096
- `attributes`: 32 per tag
- `observations`: 8 per inspection
- `version_bytes`: 32
- `version_separator_bytes`: 32

The independent UI result has no Show-on-map action. It is cleared when its exact
source/session or retained history is invalidated and is never saved in workspace
state. An unchanged valid metadata poll may preserve it; delayed responses cannot
restore stale evidence. No larger inspection runs automatically after a limit.

## Temporary body-reference overlay (GUI only)

After **View source** has verified one exact retained row, use **Inspect retained
body references** to inspect that response's supported preview. A successful
`scanned` or `partial` result with at least one other origin enables **Show
references on map**. A zero-reference or self-only result remains visible in the
inspector but cannot create an overlay; unsupported, unavailable, or unverified
results cannot be mapped.

**Show references on map** opens Direct relations, pins the inspected response's
source origin, and adds its references to the loaded metadata neighbors. It uses
only the already-returned, bounded result: showing, selecting, searching, and
clearing the overlay perform no further body read or target request. Nothing is
scanned automatically. Only one overlay exists at a time, in memory for the
current session; it is not written to saved workspace or session data.

The source method and query-free path, exact retained-row provenance, and
complete or partial retained-preview coverage remain visible. A partial scan
stays labeled partial even when the metadata inventory is complete. Dotted
**body URL reference** arrows point from that response's origin to referenced
origins. Repeated literal occurrences are reference counts, never request counts.
These arrows do not prove a request, runtime call, followed link, service
ownership, or dependency. Target origins remain text and are never fetched.

A target already present as a captured origin keeps its actual metadata and
sampled traffic count. A target present only in the inspected preview is
reference-only: no captured traffic, endpoint, or scope membership is invented.
That label does not establish that the origin has never been captured. The
overlay does not enrich All origins, the API catalog, header-derived links, or
the metadata/CLI result. The existing body-reference CLI output is unchanged;
there is no additional command or API endpoint for this GUI action.

The overlay keeps at most 100 referenced origins from the bounded scan. Self
references are excluded from the graph. The pinned source counts toward the
existing 24-card limit, and body arrows share the 40-arrow limit with metadata
arrows. Search can hide neighbors while retaining the source. Read the reported
query-hidden, card-cap, and arrow omissions; graph omissions are separate from
the scanner's omitted reference occurrences and unknown coverage beyond its
limits. Selecting a referenced target changes the inspector without moving or
reassigning the pinned source.

Use **Clear body references** to return to metadata-only Direct relations. The
overlay also clears on returning to All origins, focusing a different anchor,
hiding/leaving Service Map, a forced refresh, scope or session changes, source
loss or revision, snapshot generation changes, refresh failure, or disconnect.
Every transaction event clears it: insert and update events are not
distinguishable here, so even newly appended traffic invalidates the overlay.
Retention, removals, and event gaps likewise invalidate it. An ordinary unchanged
metadata poll preserves it only while the original session, store generation,
source origin, and exact evidence token remain present. Another token with the
same transaction UUID or method/path is not a substitute; a token falling out of
the bounded evidence sample also clears the overlay. Later polls, delayed scan
responses, and reopening the map never restore it automatically.
Showing a new overlay is disabled while the session's event stream is connecting
or disconnected, or the current metadata snapshot is stale or failed to refresh.
Metadata polling alone does not restore event readiness; the owned event stream
must reconnect and the source must be verified again.

For steadier inspection during live traffic, manually pause HTTP recording with
the **Capturing** control in Capture before verifying and inspecting the source.
Traffic still passes through the proxy. Pausing is not a snapshot freeze:
already-started recorded streams may finish updating, and explicit replay sends
and WebSocket upgrades can still add HTTP history rows. Let in-flight operations
finish before showing the overlay; other history changes can still clear it.
Resume recording when ready. Showing an overlay never pauses or resumes recording
for you.

## Local API

- `GET /api/target/service-map?session_id=<uuid>&in_scope_only=false&max_records=10000`
- `GET /api/target/service-map/evidence?session_id=<uuid>&evidence_id=<opaque-token>`
- `GET /api/target/service-map/body-references?session_id=<active-uuid>&evidence_id=<opaque-token>`
- `GET /api/target/service-map/html-generators?session_id=<active-uuid>&evidence_id=<opaque-token>`

### Technology catalog negotiation

Only the map and exact-row evidence GET endpoints accept the inbound local API
header `X-Sniper-Technology-Catalog-Version`. Its value must be exactly one `1`
or `2`. Omission selects the frozen revision-1 catalog and preserves the legacy
wire shape: there is no `technology_catalog_version` field. Explicit `1` or `2`
selects that catalog before literal parsing, token/row caps, metadata-byte limits,
and origin/map aggregation. Its successful response, including unavailable
exact-row evidence, adds top-level numeric `technology_catalog_version: 1` or
`technology_catalog_version: 2`. Other response fields and `schema_version: 1`
are unchanged. Selecting a catalog never changes exact retained-row tokens.

Duplicate header fields, comma lists, empty values, and malformed or unsupported
values receive HTTP 400 with the fixed message `invalid technology catalog
version header`; the supplied value is never echoed. There is no new API query
parameter and no outgoing target header. The body-reference endpoint and its
projection do not negotiate catalogs and remain unchanged.

Compatibility works in both directions:

- Old clients omit this header. New servers keep revision-1 products, observation
  bytes, and omission behavior, including when new products would otherwise fill
  row or aggregate caps. Expanding the catalog and then filtering is not equivalent.
- New clients request revision 2. An old server may ignore the header and omit
  the response marker. The client then validates strictly against revision 1 and
  reports legacy catalog availability; absence never means revision 2 succeeded.
  This also supports older servers that omit the observation extension itself.
- A present marker must be a supported integer equal to the requested revision.
  Malformed, unsupported, or mismatched markers are rejected before validating
  observations. Neither clients nor servers silently retry with another catalog.

The Scope UI requests revision 2 on both metadata reads. The CLI defaults to
revision 2 for map and evidence; `--technology-catalog-version 1` or the numeric
`technology_catalog_version: 1` call input explicitly requests revision 1. These
CLI options become a local request header, not query parameters. CLI dry-run
reports the selected header without contacting the server. A legacy fallback
notice is separate from the machine-readable result. Body-reference commands
send no catalog header and gain no catalog option.

URL-encode the opaque token as a query parameter. These endpoints use the existing
local API access control. Service Map discovery and reads do not follow API
redirects or retry failed requests. Discovery validates the usual Sniper identity;
when it reads local runtime-state, it preserves that file even if it is stale or
invalid and does not create lock or recovery files. A CLI schema describes the installed CLI; an older
running Sniper may not provide the Service Map endpoints. Never fall back to
active traffic generation when a read is unsupported or incomplete.
