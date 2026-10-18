# Recorded socket peer matches browser fixture

This is a synthetic, passive UI fixture. It runs the actual `web/service-map.js`
controller and unchanged `web/styles.css`. Its service-map DOM is copied directly
from `web/index.html`. It never runs `app.js`, starts Sniper, connects a proxy,
changes capture settings, performs DNS resolution, or contacts fixture origins.
All hostnames are under `example.com`; the recorded socket address is the
RFC 5737 documentation address `192.0.2.44:443`.

## Serve and open

From the repository root, use an isolated loopback-only static server:

```sh
python3 -m http.server 18897 --bind 127.0.0.1
```

Open `http://127.0.0.1:18897/tests/fixtures/recorded-peer-browser.html` in a browser.
The browser fetches only the fixture HTML/JS/CSS and real production JS/CSS from
that static server. CSP denies all connections (`connect-src 'none'`), images,
fonts, media, frames, workers and forms; scripts/styles are same-origin only.
The real controller receives a closed request adapter that returns synthetic
responses in memory. Fetch/XHR/WebSocket/EventSource guards record and reject
accidental connection attempts. No request is sent to any `/api/` endpoint.

## Main checks

1. The default view loads with one stub map GET and zero peer-match GETs.
2. Inspect the source, find recorded matches, then explicitly show them on the
   map, using the production inspector buttons or the numbered fixture helpers.
   Loading matches alone must not change the graph. Showing the overlay, query,
   zoom, selecting an origin, Clear and All origins must not fetch a peer result.
3. App, API and archived origins share recorded address values but remain separate
   origins. `archived.example.com` is absent from the default map. Show the group,
   select it and inspect its exact sample; the original source stays pinned.
4. Every synthetic row intentionally shares one valid record UUID, while its
   exact token carries a distinct string sequence above 2^53. API has two such
   samples, with descending sequences, and they must remain distinct.
5. Choose Many origins and repeat the steps. There are 34 matching origins including
   the source origin, enough to exceed the existing 24-node map cap.
   Counts, visible-node limits and omissions must not imply unknown origins were
   scanned or that equal address values establish origin ownership.
6. Use the production query, In scope only, All origins, Refresh, zoom and Clear
   controls. The query is local; scope/Refresh reads are map-only until the user
   explicitly requests a new source and match inspection.
7. Choose Partial to show the source outside the capped 10,000-row candidate window and one omitted
   exact sample. No matches, Unavailable and Malformed exercise empty, unavailable
   and fail-closed inspector results. Validate fixture DTOs checks twelve valid
   scenario/scope combinations and rejects a malformed numeric sequence.
8. Expand lifecycle controls. History changes, invalidation, changed generation,
   changed session, event disconnect and hiding the map clear temporary state.
   Hold a peer response, trigger one of these, then release it to ensure late work
   cannot resurrect a discarded overlay or result. The hold deliberately ignores
   abort so the production request identity checks are exercised.
9. Toggle Sample has become unavailable, then inspect an exact peer sample. The
   stub returns unavailable for that exact token; it never falls back to UUID.
10. Click Next peer read: source changed, then Find recorded matches, before
    Show or while a group is shown. The next stub result returns the authoritative
    `record_changed` reason. The verified source, socket peer and supplementary
    inspections must disappear; a same-source shown group must disappear; the
    fixed source-unavailable notice remains. An unrelated pinned group remains
    when the unavailable read concerns one of its matched samples. The visible
    source/peer diagnostics report whether those production inspector labels
    are present. The Unavailable scenario uses `record_missing` and must clear
    the verified source and peer in the same way.
11. Choose Scope work limit, or click Next peer read: scope work limit and then
    Find recorded matches. Arming this one-shot control alone must add no reads.
    The synthetic unavailable response has `scope_work_limit`, null source/peer/
    snapshot, empty origins and zero counts; its fixed limits also include
    `scope_pattern_entries: 500`, `scope_pattern_bytes: 512`, and
    `scope_work_units: 8388608`. Unlike a changed source, this bounded inspection
    failure must preserve the currently verified source and socket peer. Test
    before Show, with a same-source group shown, and while inspecting a different
    matched sample within that group: every still-valid shown overlay must remain.
    The inspector explains that scope matching reached its work limit. This
    fixture never retries automatically or changes scope/capture settings.
12. Inspect the visible counters and stub request log. External connection
    attempts and unexpected stub requests should remain zero. All logged tokens
    and timestamps are synthetic.

The optional `window.recordedPeerFixture` test handle exposes the real controller,
read-only observations/counters and fixture constructors for browser assertions.
No browser-only mock controller or alternate graph renderer is used.

## Refresh provenance after production changes

```sh
python3 tests/fixtures/recorded-peer-browser-manifest.py
```

This synchronizes only the fixture's production service-map DOM section, then
writes SHA-256 hashes for the production inputs and fixture files to
`recorded-peer-browser.manifest.json`. Re-run it after the final production edit.
The script does not change production files or invoke Cargo.
