# Exact source lifetime browser fixture

Synthetic, passive browser QA for `web/service-map.js`. This loads the actual
controller, unchanged production stylesheet, and the Service Map DOM copied from
`web/index.html`. There is no mock controller, application bootstrap, Sniper
process, capture, proxy, DNS lookup, or external data. Hostnames use `example.com`;
the only socket address is RFC 5737 documentation address `192.0.2.44:443`.

## Serve and open

From the repository root:

```sh
python3 -m http.server 18897 --bind 127.0.0.1
```

Open `http://127.0.0.1:18897/tests/fixtures/source-lifetime-browser.html`.
The browser loads only local static assets. CSP uses `connect-src 'none'` and
allows only data: images used by the production CSS chevrons; other images, fonts,
media, frames, workers, forms, and objects are blocked. All controller
GETs are served by a closed in-memory adapter. Fetch, XHR, WebSocket, EventSource,
and sendBeacon guards expose accidental connection attempts. No `/api/` network
request is made. A blocked local Bungee font from the unchanged production CSS
is counted separately from unexpected CSP blocks.

The default page does one synthetic map read; it does not inspect a source or a
supplement automatically. Production inspection buttons work normally. The
fixture's helpers call those same controller methods; they never edit its state.

## Authoritative source failures

1. Choose a shown overlay (Body, Link, or Recorded peer group), then click
   **1. Prepare A + shown overlay**. This switches sessions away/back through
   actual `controller.reset()` calls, verifies source A, inspects and explicitly
   shows that overlay, and loads peer and HTML supplementary results.
2. Choose Body, Link, HTML generators, or Peer matches under Unavailable response.
   Choose one of `invalid_evidence_id`, `session_mismatch`, `stale_generation`,
   `record_missing`, `record_changed`, or `session_source_changed`.
3. Click **2. Arm next response**, then **3. Run selected inspection**. Arming alone
   adds no read. The response is a valid typed unavailable DTO for that API.
4. Verified source, socket peer, supplementary results, and the same-source shown
   overlay must clear. The production source-unavailable message should describe
   the relevant evidence/session context without claiming a row was deleted when
   the response only says its session or generation changed.
5. Repeat with each response type and reason. Use Prepare between independent
   cases, including when switching the shown-overlay kind. Do not use an overlay
   helper to silently reverify the source in the middle of a case.

The exact sequences are strings above 2^53 and all rows share one UUID. Source A
and B are separate retained rows, not UUID aliases.

## Inactive selected session

1. Prepare an overlay, choose any response type, arm `inactive_session`, and run.
2. Ordinary verified source and its recorded socket peer remain. Supplementary
   results and the shown overlay clear. The production message identifies the
   selected session as inactive; all active-only inspect/show buttons are disabled.
3. Try **Select B and back to A**, **Same-session refresh**, **New generation +
   refresh**, **Scope revision + refresh**, and **Reconnect + refresh**. You can
   also use production Refresh and In scope only. Reinspect ordinary source A as
   needed after refreshes clear the inspector. The latch must remain BLOCKED,
   and direct fixture inspection calls must add zero supplementary reads.
4. Use **Switch session away and back**. These controls change the session getter
   and call the actual reset lifecycle. Reinspect A, then run a supplement; active
   inspection can now proceed. Neither selection nor a refresh fakes this reset.

**Fresh session context** and **Prepare** also perform genuine selected-session
switches and therefore intentionally release the latch. Do not use them during
step 3. Scope revision is only a simulated controller notification, not a real
configuration change.

## Optional inspection failures

Prepare each overlay and use an optional reason near the bottom of the reason
selector. Body/Link/HTML include `response_unavailable`; peer matching instead
uses its valid `observation_unavailable`, `scope_work_limit`, or
`peer_inspection_failed` reasons. `metadata_unavailable` is available on all four.
After the explicit response, ordinary verified evidence, its socket peer, and
the previously shown overlay remain. The failed optional result may be shown as
unavailable without revoking the exact ordinary source. No automatic retry or
setting change is performed.

To check an unrelated pinned overlay: prepare source A, inspect source B, arm a
row-local failure such as `record_changed`, and run it. B should clear while A's
shown overlay remains. `stale_generation` and `session_source_changed` may affect
the broader generation/session context; they are not row-local cases.

## Held response and sibling races

- **Prepare delayed A result after verified B** uses the selected response type,
  reason, and shown-overlay choice. It prepares A, holds the unavailable response,
  and verifies B. Click **Release held responses**. The delayed A result must not
  revoke B or apply an old inactive-session latch; the unrelated pinned overlay
  remains. The held diagnostic shows its aborted status before release.
- **Prepare concurrent same-source sibling** holds a successful HTML supplement
  (or peer supplement when the failing type is HTML), then delivers the selected
  unavailable result. Release the held success. An authoritative failure or
  inactive-session response must not let its canceled sibling restore results,
  source evidence, overlay, or active-only eligibility.
- For custom races, use **Hold next** with a response type, trigger the production
  read, change context, then release. Holds deliberately resolve after abort, so
  production identity/request fences are exercised. Responses are snapshotted
  when requested, not reconstructed in the new context when released.

To check newer inspector navigation, hold an inspection response, then click the
production **APIs** inspector tab before releasing it. Authoritative failures and
inactive-session replies must keep APIs selected; a canceled sibling released
later must also preserve that choice. Return to **Evidence** to see the stored
fixed failure message. Inactive-session replies retain ordinary verified metadata
and show the active-only eligibility notice globally while APIs remains selected.

The visible diagnostics report current source, socket peer, overlay provenance,
inactive latch, each supplement, pending work, armed reasons, actual production
button availability, connection counters, and held responses. The request log
contains synthetic identifiers only.

## Exact-row technology field

Choose **Absent: unavailable**, then inspect A or B. Its exact source should say
header technology observations are unavailable, rather than claiming no supported
declarations. Choose **Present []: inspected-empty** and inspect again; this says
no supported declarations were found. **Present: nginx declaration** restores a
literal synthetic nginx 1.26.2 observation. These modes apply only to exact rows;
map aggregates independently remain inspected-empty.

## Offline checks and provenance

After the final production edits, run:

```sh
python3 tests/fixtures/source-lifetime-browser.manifest.py
node --check tests/fixtures/source-lifetime-browser.js
node tests/fixtures/source-lifetime-browser.check.cjs
```

The manifest script syncs only this fixture's copied map DOM and hashes the
production inputs and fixture files. It never edits production code or runs
Cargo. Checks validate 376 valid supplement DTO variants across two sessions,
two generations, and sources A/B, plus absent/empty/declared technology modes,
scoped peers, malformed DTO rejection, local assets, control IDs, exact production
DOM, CSP, and file hashes. Static checks are not a replacement for real browser QA.
The visible **Validate fixture DTOs** button runs the same data-contract matrix
without adding reads.

`window.sourceLifetimeFixture` exposes the real controller, factories, counters,
request log, `observations()`, and helpers (`prepare`, `arm`, `inspect`,
`inspectSource`, `show`, `changeGeneration`, `reconnect`, `switchAwayBack`,
`delayedSourceA`, `siblingRace`, `releaseHeld`) for browser assertions.
`require('./tests/fixtures/source-lifetime-browser.js')` exports only the data
factories in Node, with no DOM, app bootstrap, request adapter, or networking.
