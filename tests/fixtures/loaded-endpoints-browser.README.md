# Loaded endpoint filters and navigation browser fixture

This isolated synthetic preview runs the actual `web/service-map.js`, actual
`web/styles.css`, and the service-map DOM copied from `web/index.html`. It never
loads `app.js`, runs Sniper, opens a proxy, contacts any real API or fetches any
fixture origin. All hostnames are under `example.com`. CSP blocks connections,
external images, fonts, frames, media and workers. Only `data:` images are allowed
so the production CSS select chevrons render without a network request. Fetch,
XHR, WebSocket and EventSource guards
record and reject accidental connection attempts. The controller's request
adapter returns only synthetic map/evidence DTOs in memory.

Serve the repository with a loopback-only static server, for example:

```
python3 -m http.server 18898 --bind 127.0.0.1
```

Open `http://127.0.0.1:18898/tests/fixtures/loaded-endpoints-browser.html`.

## Checks

1. Initial inspector is `app.example.com` with 130 loaded endpoints. Use its
   production Next button twice, open `/path-110`, View source, then APIs.
   Page 3 and exact selected endpoint must remain. Repeat using keyboard tabs.
2. API catalog has independently retained pagination. Select an endpoint from
   a later catalog page, return via API catalog, and verify the same page.
3. Select the Reordered snapshot and Refresh selected snapshot. `/path-110`
   moves to index 0; its exact selection remains and inspector page becomes 1.
   A page without endpoint selection follows its first-row identity on refresh.
4. Method is exact and case-sensitive: `GET`, `get` and the long unknown method
   are distinct choices. Path contains is a literal, case-sensitive substring:
   `CaseSensitive` and `casesensitive` differ; `[literal]` is literal text.
5. Filters apply to the two API lists only. Graph cards, origin search and zoom
   do not change. All local filtering/paging/tab actions add zero stub reads.
   Counts identify matches, loaded combinations and locally hidden combinations;
   the partial scenario separately discloses server bounds/omissions.
6. Select and verify an endpoint, filter it out in the catalog, then return to
   the inspector. Evidence and selection stay intact with an outside-filter
   notice. Zero matches must not imply missing server data or a full inventory.
7. Select `get` or the long custom method, then refresh the Remove get and custom
   method snapshot. The selected filter remains, with an explicit empty-state
   notice. It must not silently switch to All methods.
8. Test typing/editing a path substring with the caret in the middle. Focus and
   selection must survive result redraws. Clear resets both local filters.
9. Use 20 px maximum display size and a narrow inspector. The long method and
   long path must stay bounded; filters must not enlarge Scope/navigation chrome
   or inflate graph cards. Filter labels, native selector and input remain usable.
10. Hold an exact evidence response, change a local filter or API tab, then
    release. The same exact source may finish without changing the active tab.
    Hold another, invalidate history, then release: stale evidence must not return.

Visible diagnostics show exact selection, pages, filters and stub/blocked-read
counts. The read-only `window.loadedEndpointsFixture` test handle exposes the
actual controller plus synthetic fixtures and diagnostics. Browser checks should
leave unexpected requests, external connection attempts and unexpected CSP blocks
at zero. The production stylesheet may try its local `/fonts/Bungee-Regular.ttf`;
`font-src none` deliberately blocks that resource. The fixture reports those
exact same-origin font violations separately as expected local font blocks; their
count can vary by browser font use. Total CSP blocks may therefore be nonzero.
Other CSP violations remain unexpected and must be investigated.

After production changes, run `python3 tests/fixtures/loaded-endpoints-browser-manifest.py`
to resynchronize the production DOM and refresh the fixture SHA-256 manifest.
