# Recon endpoint discovery

Recon builds one endpoint inventory per session from three sources: a static crawler of the target, existing urlscan results, and the Wayback CDX index. Open the **Recon** tab, enter an absolute target URL, choose sources and optional budgets, then start. Each source reports its status, request count, discoveries, partial results and warnings. **Stop** cancels the job's local work. Jobs stay with their session when you switch sessions; deleting the session or shutting Sniper down cancels them. After a restart, unfinished jobs are marked interrupted and do not resume.

## Scope and consent

Use Recon only against targets you are authorized to assess. By default, scope is the target's exact hostname, scheme and effective port; the subdomain checkbox adds the domain's subdomains. HTTP on port 80 may upgrade to HTTPS on port 443, never the reverse. Other hosts and ports are out of scope, and paths do not narrow it. Every URL the crawler follows and every provider result is checked against scope. WebSocket URLs may be stored but are never connected to.

The crawler sends unauthenticated GET requests only, with TLS validation and the session's upstream proxy. It follows ordinary links, resource references and sitemap entries. It does not submit forms, run JavaScript, send captured cookies or `Authorization` headers, or visit archived URLs and JavaScript candidates. GET requests can still have side effects on some sites. robots.txt rules are kept as candidates and not fetched.

History lookup sends the target hostname to urlscan.io and/or web.archive.org, so it needs an acknowledgment. It does not create a urlscan scan.

**New urlscan scan** needs its own acknowledgment and a visibility: private (default), unlisted or public. Unlisted is not private. urlscan loads the target in its own browser, which may load resources outside Sniper's scope. The full target URL is submitted, so URLs with a query string, fragment or credentials are rejected; never submit a confidential path. Sniper does not fall back to another visibility and does not retry a submission with an uncertain outcome. Stop cannot cancel a scan urlscan already accepted.

Set `URLSCAN_API_KEY` in the environment of the Sniper process for authenticated urlscan search or submission. The key never reaches the UI, session state, logs, Archive.org or the target. History lookup may work without a key, depending on provider policy. Provider requests use their own direct connection and never carry captured credentials or the upstream proxy's credentials.

## Evidence

Entries are deduplicated by normalized URL plus method. The query string is kept as is, fragments are ignored, and the raw spellings are kept separately. Each observation has one evidence kind:

- live: the crawler received an HTTP response, including an error response
- observed: a literal reference in a page, or a result of a urlscan scan you submitted; the URL may not be available now
- history: a past urlscan or Wayback observation
- candidate: a JavaScript URL literal, a form action and its fields, or a robots rule; never visited automatically

## Budgets

`max_requests`, `max_depth`, `max_endpoints` and `max_pages` default to 0, which means no limit; a positive value limits one job. `max_endpoints` counts new entries the job adds; existing entries can still gain evidence. `max_pages` applies to each history source separately. A job that stops at a budget with work left is reported as partial. `max_depth=0` used to mean the seed only; it now means no limit.

Without budgets nothing caps a job, so a broad or growing target can use a lot of memory and disk. Set a budget or use Stop.

## What each source collects

- urlscan: full scan results, including request and response URLs, links, redirects and WebSocket observations.
- Wayback: CDX records without status or MIME filters, so error responses and old resources appear too.
- Crawler: HTML links, resources and base URLs; GET and POST form actions and field names; URL literals in inline and external JavaScript; robots hints; and sitemap indexes and URLs. It does not render pages, log in, look for source maps, evaluate JavaScript, expand wildcards or infer dynamic routes.

## Scope site map

**Scope → Site map** includes the active session's Recon inventory, read-only. Rows mark captured and discovered evidence and keep the Recon source and evidence kind. A row that was also captured keeps its HTTP status and capture metrics, and `request_count` counts captured traffic only. Discovered endpoints do not become proxy transactions, so a Recon-only row has no request to open or replay. The inventory is saved with its session and stays in the map after a restart.

## Local API

- `GET /api/recon?session_id=<UUID>` returns the session ID, jobs and inventory.
- `POST /api/recon/jobs` takes `session_id`, `target_url`, `include_subdomains`, `sources` (`crawler`, `urlscan`, `wayback`, `urlscan_submit`), and the optional budgets above. History sources need `history_acknowledged`; `urlscan_submit` needs `scan_acknowledged` and `scan_visibility`.
- `POST /api/recon/jobs/<job UUID>/stop` with `{"session_id":"<UUID>"}` cancels a job and returns `{"stopped":true,"session_id":"<UUID>"}`.

There is no `sniper-cli` command for Recon yet. The API uses the same authentication as the other routes.

## Validation

Tests use local fixtures and loopback servers only. After a rebuild, run:

```bash
cargo fmt --all -- --check
cargo test --locked --release
node --check web/recon.js
node --test tests/recon_ui.test.cjs
python3 tests/recon_discovery_smoke.py target/release
```

Smoke-test the desktop app before a release.

## Provider references

- https://docs.urlscan.io/pages/search-api-reference
- https://docs.urlscan.io/pages/result-api-reference
- https://urlscan.io/docs/api/
- https://github.com/internetarchive/wayback/tree/master/wayback-cdx-server
