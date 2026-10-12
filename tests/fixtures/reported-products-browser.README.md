# Reported product and literal-version browser fixture

This isolated preview runs the actual `web/service-map.js`, actual
`web/styles.css`, and `targetServiceMapPanel` extracted from `web/index.html`.
It never loads `app.js`, runs Sniper, opens a proxy, reads any session directory,
contacts an API, or fetches an origin. All hostnames are under `example.com`.
The production controller receives a closed in-memory request adapter. The only
accepted synthetic requests are same-session GET map/evidence/peer-match reads with
`redirect: 'error'`. Other operations fail visibly.

CSP sets `connect-src 'none'` and blocks external images, fonts, frames, media,
objects and workers. Only local scripts/styles and `data:` CSS images are allowed.
Fetch, XHR, WebSocket, EventSource and sendBeacon guards reject accidental browser
connections. A synthetic evidence button reads one in-memory DTO; it never
contacts the displayed hostname. Do not run Cargo or the real application for
this fixture.

## Open the fixture

From the repository root, use an approved loopback-only static server, e.g.:

```sh
python3 -m http.server 18898 --bind 127.0.0.1
```

Open `http://127.0.0.1:18898/tests/fixtures/reported-products-browser.html`.
This is a fresh fixture; it does not change the loaded-endpoints or recorded-peer
fixtures. After production UI edits, synchronize its DOM and hashes:

```sh
python3 tests/fixtures/reported-products-browser-manifest.py
```

## Deterministic synthetic cases

The default v2 snapshot contains 29 origins and 354 endpoint combinations,
including 130 on `app.example.com`. Its 29 retained redirect instructions point
to the other 28 captured origins and `reference-only.example.com`, producing 30
graph origins before filtering. Every redirect has its own exact primary endpoint
source (paths 50–78), with matching 302 evidence. Graph-card bounds can be tested
independently of matching origin counts. Each retained sequence is unique to an
origin and each aggregate uses exactly the same complete token as its endpoint.
Exact-source rows carry validated header-index/UTF-8-span provenance. There are
no arbitrary product names, raw header text, bodies, discovery results, or inferred
software stacks.

- `app.example.com`: nginx `1.24.0`, `1.26.2`, and unknown/not reported. Conflicting
  reported literals coexist with an unknown observation.
- `api.example.com`: Gunicorn `22.0.0` and `23.0.0`, exercising a v2-only product.
- `all-unknown.example.com`: nginx not reported and unsupported format, plus
  Apache-Coyote non-product version. No reported literal is present.
- `known-and-unknown.example.com`: Caddy `2.8.4` plus not reported. One reported
  literal plus an unknown state must not count as multiple reported versions.
- `same-version-two-headers.example.com`: PHP `8.3.12` on server and x-powered-by.
  Repeated identical literals must not count as multiple reported versions.
- `literal-leading-zero.example.com`: nginx `01.2` and `1.2`. Literal strings stay
  distinct; filters must not coerce them into one numeric version.
- `different-products.example.com`: nginx `1.24.0` and Caddy `2.8.4`. Different
  products do not create a same-product conflict.
- `empty-extension.example.com` has an explicit empty observation array;
  `missing-extension.example.com` omits the field. Both have a legacy nginx clue,
  which must never become a structured product/version match.
- All 20 catalog v2 products are represented. Three `zz-` origins exercise matches
  beyond the initial 24-card cap, including a long hostname with the exact maximum
  32-byte reported version `12345678901234567890123456789012`.
- Endpoint cases preserve exact `GET` versus `get`, a long custom method, long
  path, literal `[literal]`, and case-sensitive path variants.

## Browser checks

1. Open the production Products / versions disclosure. Product and version are
   independent local selectors. Exercise Any, nginx, a v2-only product, each
   unknown state, an exact reported literal and Multiple reported versions.
   Options come from the loaded default aggregate inventory, independently of
   origin search and currently selected endpoint filters. In scope only performs
   its existing map read and builds options from the newly loaded snapshot.
2. nginx plus `1.26.2` matches three origins: app and the two beyond-card-cap
   origins. nginx plus `1.24.0` matches app, different-products and node-23.
   No default match should be limited to the 24 rendered cards. Combine product
   selection with origin search and scope, then clear local filters.
3. With no product selected, Multiple reported versions matches app, api and
   literal-leading-zero only. nginx plus Multiple matches app and
   literal-leading-zero. PHP's duplicated literal and Caddy's known+unknown do
   not match Multiple. Unknown states keep their explicit provenance labels.
4. Product/version matches apply consistently to map origins and API catalog
   origin groups. Keep the selected inspector/source when a local filter hides
   its origin, with an outside-filter explanation. Confirm graph card shape,
   evidence meaning, exact source token and zoom are not changed by filters.
5. All local product/version, origin search, method/path, pagination and
   map/catalog/tab choices add zero stub reads. In scope only intentionally
   reloads a synthetic scope-filtered snapshot (22 origins, 298 combinations).
   Scope keeps 354 retained/considered rows and reports 56 excluded rows; partial
   projections keep scope exclusions separate from their 9,646 omitted rows. Product filters must not trigger
   HTML body reads, peer lookup, scan, replay, capture or any other operation.
6. Select Gunicorn, then choose Remove all Gunicorn declarations and refresh.
   The selected product stays explicit and missing, with no silent reset.
   Repeat with nginx plus `1.26.2` and Remove all nginx 1.26.2 declarations.
   Empty and missing-field snapshots provide broader disappeared-option cases.
7. Select Unmarked catalog v1 fallback and refresh. The response omits the
   catalog marker and contains only v1 observations. The legacy explanation is
   visible; no v2-only declaration or old legacy clue becomes a product match.
8. Partial server projection keeps 29 origins and 354 loaded combinations but
   reports record/endpoint caps and omitted rows. Local hidden/matched totals
   must not claim a complete inventory. A zero-match filter is distinct from a
   complete empty server response.
9. On app, inspector APIs → Next → Next → `/path-110` → View source → APIs.
   Exact endpoint selection and page 3 persist. Apply product/version filters
   and local endpoint filters without losing exact-source binding. Refresh the
   Reordered snapshot: `/path-110` moves first; its identity persists and page
   becomes 1. Catalog pagination stays independent.
10. Hold next evidence read, click View source, change a local filter or tab,
    then Release held. The same exact source may complete without changing the
    active tab. Hold another read, Invalidate history, then release: stale
    evidence must not return. A held map read likewise permits refresh-race
    testing; the adapter intentionally releases even aborted responses. Peer-match
    reads can be held with the same control after an explicit production request.
11. Keyboard-operate the disclosure, native selectors and clear button. Test
    8, 12 and 20 px text at normal and narrow widths. Inspect long hostname
    selects the long origin; choose its 32-byte version. Production controls
    must stay bounded, usable and legible without enlarging unrelated chrome.

## Direct relations and explicit peer projection

- Select app, then Direct relations. Its pinned source has 29 metadata neighbors,
  including the reference-only origin. Filter to nginx and a literal version to
  check that matching happens before the card cap and keeps the pinned source.
  The reference-only card must show product information unavailable and cannot
  match a structured product filter. Redirect instructions are retained metadata;
  no request, call or origin ownership is inferred.
- In app's APIs, select `/path-0`, then its ordinary View source button. The exact
  row has an observed direct socket peer `192.0.2.10:443` (a documentation address).
  Merely opening the fixture, selecting a source, or changing product filters
  does not inspect peers. Use the production Find matching recorded peer button
  for exactly one synthetic peer read, then Show recorded address group locally.
- The result contains 30 matched origin groups: app's second exact row, the 28
  other default origins, and `peer-only.example.com`. It therefore exceeds the
  24-card graph limit. The peer result deliberately carries no product fields.
  Default nginx and different-product cards must join their existing aggregate
  summaries by exact origin. The peer-only origin has no default aggregate and
  must show product information unavailable, never a product inferred from its
  address group or another origin. Product filtering remains local.
- The peer projection independently reports 355 retained metadata rows: the 354
  default endpoint rows plus one peer-only sample. Of these, 31 have the matching
  observed address. With app `/path-0` as the inspected source, the 30 remaining
  rows match; the other 324 rows have no recorded peer. Scope excludes seven of
  those matching rows. The default loaded endpoint count stays 354.
- The Partial server projection preserves its separate 20,000 retained / 10,000
  considered peer window, with app's source outside that window. Its explicit
  reasons and counts are validated rather than claiming a complete inspection.
  The same nine map scenarios remain available; Empty yields unavailable peer
  data. Peer-only exact sample evidence can be opened using production controls.
- Hold a peer-match read, change product filters and release to test current
  source preservation. Repeat with Invalidate history before release to verify
  stale matching data cannot recreate a cleared group. Watch the separate peer
  read counter; every connection-attempt counter must remain zero.

The visible counters report in-memory reads, held responses and blocked
connection attempts. The peer counter advances only for the explicit peer action.
The selection output includes product/version, exact origin,
endpoint, page and verified source sequence. Inspect the synthetic request log
when checking which action caused a read.

Unexpected requests, connection attempts and unexpected CSP blocks must stay
zero. Production CSS may attempt its local `/fonts/Bungee-Regular.ttf`;
`font-src 'none'` deliberately blocks exactly that resource and reports it as an
expected local-font block. Its count depends on browser font use; other CSP
violations are unexpected.

`window.reportedProductsFixture` exposes the actual controller, factories and
inert diagnostics for browser QA. The same JS exports data-only factories under
Node, permitting production-normalizer validation without any DOM or server:

```sh
node --check tests/fixtures/reported-products-browser.js
node - <<'NODE'
const assert = require('node:assert/strict');
require('./web/service-map.js');
const fixture = require('./tests/fixtures/reported-products-browser.js');
for (const scenario of fixture.SCENARIOS) {
  const payload = fixture.buildSnapshot(scenario);
  const normalized = globalThis.SniperServiceMap.normalizeSnapshot(payload, fixture.SESSION);
  for (const origin of payload.origins) for (const endpoint of origin.endpoints) {
    const evidence = fixture.buildEvidence(endpoint.evidence[0], scenario);
    assert.equal(evidence.row.origin, origin.origin);
    assert.equal(evidence.row.sequence, endpoint.evidence[0].split('.')[3]);
    globalThis.SniperServiceMap.normalizeRowTechnologies(evidence.row.technology_observations, normalized.technologyCatalogVersion);
  }
}
console.log('All synthetic snapshots and exact-source technology rows normalize.');
NODE
```

The manifest hashes production sources plus every fixture source file. Regenerate
it after the final production edit before browser QA or archival; it records the
DOM/controller/styles used, not a separate implementation of those components.

## Bounded ordinary-map refresh failures

Open the fixture-only Bounded map refresh checks disclosure. The original
successful product, endpoint, exact-source and peer scenarios stay unchanged.

1. Verify app `/path-0`, optionally Find matching recorded peer and Show recorded
   address group. Arm 429 busy, then One background poll. Exactly one map read
   fails; the previous map is visibly stale, automatic refresh is paused and the
   production Refresh button remains available. Existing verified source remains;
   the conservative failure cleanup clears temporary peer inspection/overlay.
2. Repeat for 422 scope work limit, all three 409 changes and 500 build failed.
   Each has fixed explanatory text. A 409 additionally clears exact evidence and
   observed peer data, including any current temporary address overlay.
3. After any failure, 10 background polls, Map / inspector, API catalog and
   Simulate Target navigation must add zero map reads. Local origin, product,
   version, method and path filters also add zero reads and keep pause intact.
4. The production Refresh button makes one new normal map read, succeeds because
   the armed failure was one-shot, and clears pause/staleness. In scope only also
   resumes after an actual checkbox change. Reset fixture models session reset.
5. Arm Unknown marker, Duplicate marker, Status mismatch or Missing header.
   Each displays the same fixed generic failure and pauses. Error body reads and
   connection attempts must remain zero; no synthetic private diagnostic appears
   in the production error text.
6. While paused, Invalidate history or Disconnect / reconnect clears invalidated
   displayed data but preserves pause and adds zero reads. Refresh then recovers.
7. Arm a 409, enable Hold next map read, and One background poll. Before Release
   held, click production Refresh for a newer success. Release held must not
   restore pause, clear newer evidence or replace the new map. The adapter
   intentionally returns the old failure even if its signal was aborted.

The diagnostics expose refresh paused, stale, armed failure and error-body-read
counts. Request log entries label the synthetic response case; arming alone is
local. The read-only fixture handle adds MAP_FAILURES and nextMapError for QA.
No new production DOM, CSS or controls are introduced by this fixture extension.
