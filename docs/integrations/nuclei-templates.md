# Nuclei templates

Sniper can use the community detection templates of
[projectdiscovery/nuclei-templates](https://github.com/projectdiscovery/nuclei-templates)
(MIT license). Choose the **Templates** tool, download the latest release or
import a release archive, and use templates two ways:

- **Passive**: turned-on templates check captured traffic and never send a
  request.
- **Template Scan**: on demand, Sniper sends the templates you pick to the
  targets you name, and keeps what matched.

Sniper implements a subset of the Nuclei template language in Rust. It does not
run the `nuclei` program and does not need it installed.

## Passive templates

A template runs passively when it can judge a response it did not send:

- **Global matchers** (`global-matchers: true`): matchers meant for every HTTP
  response, such as WAF detection or secret patterns.
- **Captured GET**: one `GET {{BaseURL}}` request, with no headers, body,
  payloads or random values. Its matchers judge each captured GET response as if
  the template had sent that request, as Nuclei's passive mode does. Redirects
  need no following: each hop the client followed is its own captured response.

A turned-on template checks a captured response only when all of these hold:

- the session's **Passive Scan** is on;
- the response's host matches a Scope include pattern, and the include list is
  not empty (with no includes, templates check nothing);
- the response is not an image, font, audio or video.

Findings appear in **Capture ▸ Findings** under the category `nuclei`, with rule
ids `nuclei-passive:<template-id>` or `nuclei-passive:<template-id>:<matcher>`
for a named matcher. A template's extracted values are the finding's evidence.
Turning templates on or off affects traffic captured afterwards; **Rescan** in
Findings applies the change to the retained history. Template findings are saved
with the session; rows recovered from the journal after a crash get only the
built-in findings until a Rescan.

When many responses arrive at once, at most four template evaluations run at the
same time; a response beyond that skips them rather than slow capture, and the
library status counts it (`passive_skipped_under_load`). Rescan covers those.

## Template Scan

Template Scan sends a template's requests to targets you name, when you ask.
Nothing is sent automatically, and nothing is sent to a host you did not name
or put in Scope.

### What it sends

A template can be sent when it has one HTTP request block made of:

- a method (not `CONNECT`), and paths that start with `{{BaseURL}}` or
  `{{RootURL}}`;
- headers, a body, `variables`, `{{randstr}}`, and the helper functions,
  including the random and time ones;
- `redirects`, `host-redirects`, `max-redirects` and `stop-at-first-match`.

The target supplies the scheme, host and port; a template only adds a path. So
`{{BaseURL}}@example.net` asks the target for `/@example.net`, and a `Host`
header may only be `{{Hostname}}`, which every request carries anyway. A path
character a request line cannot hold is percent-encoded, as Go sends what Nuclei
builds; the rest is sent as written, `%2e%2e` and `..;` included.

Each request carries `User-Agent: Mozilla/5.0 (compatible; Sniper/<version>)`,
`Accept: */*` and `Accept-Language: en` unless the template sets its own.

### Where it sends

- A target is an `http://` or `https://` origin, optionally with a path the
  application lives under (`https://app.example.com/shop`), without
  credentials, query or fragment; at most 20 per scan. `{{BaseURL}}` keeps the
  path, `{{RootURL}}` leaves it out.
- Every target's host must match a Scope include pattern, and the include list
  must not be empty. Sniper checks this when the scan starts and again before
  each target's requests, so narrowing Scope stops a scan's further requests to
  a host it dropped.
- Sniper's own proxy and UI listeners, and its `sniper` page, are never targets,
  by address or by a name that resolves to this machine.
- Redirects are followed only when the template asks (`redirects` or
  `host-redirects`), at most ten hops, and only while each hop stays in Scope
  and away from Sniper. A hop that leaves is not followed; its redirect is the
  answer judged, and the job counts it (`redirects_not_followed`). Hops follow
  Go's client, as Nuclei's do: 301 to 303 become a GET without the body, 307 and
  308 repeat the request, `Authorization` and `Cookie` headers stay with their
  host, and cookies a hop sets go with the next hop to the same host.
- With **Apply Match & Replace rules** on, the session's request rules apply to
  the scan; a request a rule would send to another scheme or host is not sent
  and counts as not sent (`blocked`). Off by default.

### What it leaves out

Templates tagged `intrusive`, `default-login`, `bruteforce`, `token-spray`,
`credential-stuffing`, `fuzz`, `fuzzing`, `dast` or `osint`, and templates that
send `PUT`, `DELETE` or `PATCH` (`method:put`, `method:delete`,
`method:patch`), are left out unless you allow each risk by name. Templates
tagged `dos` are never sent.

### Pace, limits and stopping

- A **preview** counts the templates and the requests they send before any
  redirect, says which targets are refused and why, and lists what risk left
  out. It sends nothing to a target.
- A start needs an explicit acknowledgment: the checkbox under **Preview** in
  the app, `"acknowledged": true` through the API, `--yes` through the CLI.
- Requests a second, across the scan: 1 to 50, default 10. Templates in flight
  at once: 1 to 8, default 4. A template's paths go one after another.
- A scan sends at most `max_requests` requests (1 to 50,000, default 10,000),
  redirect hops included, and refuses to start when its planned requests are
  already over that.
- A target that fails 20 requests in a row is given up for the rest of the scan.
- One scan runs at a time per session; it keeps running if you switch sessions.
  **Stop** drops the requests in flight at once. Quitting Sniper, updating it, or
  deleting the session stops the scan; after a crash, a scan that was running is
  listed as `interrupted`.

### What it keeps

- HTTP history keeps only the exchanges a template matched, each noted
  `Sent by Template Scan.` and with the template and the scan's id. The rest
  appear only in the scan's request log: the latest 200 requests with method,
  URL, status, time and error, never headers or bodies.
- Findings appear under the category `nuclei` with rule ids
  `nuclei:<template-id>` or `nuclei:<template-id>:<matcher>`. They are listed
  once per template, matcher and endpoint, since a reflected random value
  differs on every run. **Rescan** keeps them: it sends nothing, so it could not
  make them again.
- A scan's results (at most 1,000), counts and warnings are saved with the
  session, ten scans at most.

A redirect chain is judged from its last answer back, as Nuclei walks it; each
answer that matches is a result, and `stop-at-first-match` ends the template at
its first match.

## What never runs

Templates are data. Sniper lists every template but never runs:

- the `code`, `javascript`, `headless`, `network`, `dns`, `file`, `ssl`,
  `websocket` and `whois` protocols, `flow`, self-contained templates and
  workflows;
- anything outside the supported subset.

A template either compiles completely or is listed as unsupported with a reason,
such as `protocol:dns`, `req:raw` or `fn:mmh3`. Sniper never runs part of a
template.

Not supported yet, and so not sent: raw requests, `payloads`, templates with
more than one request block or that read an earlier request's response
(`status_code_2`), `req-condition`, out-of-band (`{{interactsh-url}}`)
templates, fuzzing, race and pipeline requests.

## Supported subset

- Matchers: `word`, `regex`, `status`, `size`, `binary`, `dsl`, with `part`,
  `condition`, `negative`, `internal`, `name`, `case-insensitive`,
  `encoding: hex`, `match-all` and `matchers-condition`. A scan's matchers can
  also read the request (`part: request`) and the values the request was built
  with.
- Extractors: `regex` (with `group`), `kval` (response headers and cookies) and
  `dsl`. They run before the matchers, and a named extractor's first value is
  available to them, as in Nuclei.
- The expression language with govaluate's typing and the helper functions
  community templates use, including `compare_versions` with go-version rules.
- Regular expressions with Go's meaning: `\d`, `\w`, `\s` and `\b` are ASCII, and
  a `{` that does not start a repetition is a plain character.
- Responses as Nuclei presents them: the status line and Go-canonical header
  names in `header`, header values by lower snake name (`content_type`), and the
  response the server sent when a Match & Replace rule rewrote it.
- `{{Hostname}}` is the host with its port only when the URL names one;
  `{{Host}}` is the bare host.

A passive matcher judges at most 2 MiB of a body, a scan's at most 10 MiB. When
a body was truncated, templates whose result missing bytes could change (a
negated body matcher, a size, an expression reading the body) skip that
response.

On release v10.5.0, of 11,698 HTTP templates, 2,084 can run passively and 6,429
can be sent by Template Scan.

## The library

The library is shared by every session and lives under
`<data dir>/templates/`:

- `templates.json`: the installed release and the passive templates turned on;
- `bundle-<sha>.ndjson`: every template's text, written once per import;
- `index-<sha>-e<engine>.json`: what each template is and whether Sniper can run
  it, rebuilt when Sniper's engine changes.

Archive paths are labels only; nothing from an archive becomes a file name.

### Download

A download asks GitHub for the latest release tag (one request to
`api.github.com`), then fetches the release's checksums file and its source
`.tar.gz` from `github.com`. It uses the environment's proxy settings, as the
self-updater does, and follows redirects only to GitHub hosts over HTTPS. The
archive must match the SHA-256 the release publishes for it, or nothing is
installed. There is no automatic update.

The download needs an explicit acknowledgment: in the app, the checkbox above
**Download latest**; through the API, `"acknowledged": true`; through the CLI,
`--yes`.

### Import

For networks that cannot reach GitHub, import a release `.tar.gz` of at most
64 MiB. An imported archive is not checksum-verified, and the library says so.

Archives are read in memory with limits: 512 MiB unpacked, 100,000 entries,
30,000 templates and 2 MiB per template. Only regular `*.yaml` files outside
dot-directories are kept; a template id that repeats keeps its first file.

## API

| Method and path | Purpose |
| --- | --- |
| `GET /api/templates/library` | Status: installed release, counts (`passive_supported`, `active_supported`), passive templates on, any download or import in progress (`state`: `loading`, `idle`, `downloading`, `importing`, `failed`). |
| `POST /api/templates/library/check` | Ask GitHub for the newest tag; returns `latest`, `current`, `update_available`. |
| `POST /api/templates/library/download` | Body `{"acknowledged": true}`. Starts a download; 202, or 409 while busy. |
| `POST /api/templates/library/import` | Body: the `.tar.gz` (`application/gzip`). Starts an import; 202, or 409 while busy. |
| `GET /api/templates` | List; query `search`, `severity` (comma-separated), `protocol`, `support` (`passive`, `active`, `unsupported`), `tag`, `enabled`, `limit` (at most 30,000), `offset`. Each template says `requests`, what one scan run sends a target. |
| `GET /api/templates/:id` | One template with its description, references and YAML. |
| `POST /api/templates/passive` | Body `{"ids": [...], "enabled": bool}` or `{"filter": {...list query...}, "enabled": bool}`. Ids that cannot run passively are ignored and counted. |
| `POST /api/templates/scans` | Body `{"session_id", "targets": [...], "ids": [...]` or `"filter": {...list query...}, "allow_risks", "rate", "concurrency", "max_requests", "apply_match_replace"}` plus `"preview": true` to count, or `"acknowledged": true` to start. A start returns the scan; 409 while one runs. |
| `GET /api/templates/scans?session_id=` | The session's scans, newest first, without results or request logs. |
| `GET /api/templates/scans/:id?session_id=` | One scan with its results (each naming the matched history record) and its latest requests. |
| `POST /api/templates/scans/:id/stop` | Body `{"session_id"}`. Stops the running scan. |

## CLI

```bash
sniper-cli templates library status
sniper-cli templates library check
sniper-cli templates library download --dry-run
sniper-cli templates library download --yes
sniper-cli templates library import --file nuclei-templates.tar.gz --yes
sniper-cli templates list --support passive --search wordpress
sniper-cli templates get --id tech-detect
sniper-cli templates passive set-enabled --enabled true --id tech-detect --yes
sniper-cli templates passive set-enabled --enabled false --tag waf --yes
sniper-cli templates passive set-enabled --enabled true --all --yes
sniper-cli templates scan preview --target https://app.example.com --severity high,critical
sniper-cli templates scan start --target https://app.example.com --severity high,critical --yes
sniper-cli templates scan start --target https://app.example.com --id git-config --rate 5 --yes
sniper-cli templates scan list
sniper-cli templates scan get --id <scan-id>
sniper-cli templates scan stop --id <scan-id> --yes
```

`download` and `import` wait until the release is installed and print the final
status. `passive set-enabled` takes exactly one of ids, a filter or `--all`; a
scan takes exactly one of ids or a filter, and `--allow-risk` once per risk.
`scan start` returns when the scan starts; `scan get` follows it. The scan
commands use the active session unless `--session-id` names another.
