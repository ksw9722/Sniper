# Nuclei templates

Sniper can use the community detection templates of
[projectdiscovery/nuclei-templates](https://github.com/projectdiscovery/nuclei-templates)
(MIT license). Choose the **Templates** tool, download the latest release or
import a release archive, and turn on the templates you want. Turned-on templates
check captured traffic passively: they never send a request.

Sniper implements a subset of the Nuclei template language in Rust. It does not
run the `nuclei` program and does not need it installed.

## What runs, and where

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

## What never runs

Templates are data. Sniper lists every template but never runs:

- the `code`, `javascript`, `headless`, `network`, `dns`, `file`, `ssl`,
  `websocket` and `whois` protocols, `flow`, self-contained templates and
  workflows;
- anything outside the supported subset below.

A template either compiles completely or is listed as unsupported with a reason,
such as `protocol:dns`, `req:raw` or `fn:mmh3`. Sniper never runs part of a
template.

## Supported subset

- Matchers: `word`, `regex`, `status`, `size`, `binary`, `dsl`, with `part`,
  `condition`, `negative`, `internal`, `name`, `case-insensitive`,
  `encoding: hex`, `match-all` and `matchers-condition`.
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

A matcher judges at most 2 MiB of a body. When a body was truncated, templates
whose result missing bytes could change (a negated body matcher, a size, an
expression reading the body) skip that response.

On release v10.5.0, 2,084 templates can run passively out of 11,698 HTTP
templates.

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
| `GET /api/templates/library` | Status: installed release, counts, passive templates on, any download or import in progress (`state`: `loading`, `idle`, `downloading`, `importing`, `failed`). |
| `POST /api/templates/library/check` | Ask GitHub for the newest tag; returns `latest`, `current`, `update_available`. |
| `POST /api/templates/library/download` | Body `{"acknowledged": true}`. Starts a download; 202, or 409 while busy. |
| `POST /api/templates/library/import` | Body: the `.tar.gz` (`application/gzip`). Starts an import; 202, or 409 while busy. |
| `GET /api/templates` | List; query `search`, `severity` (comma-separated), `protocol`, `support` (`passive`, `active`, `unsupported`), `tag`, `enabled`, `limit` (at most 30,000), `offset`. |
| `GET /api/templates/:id` | One template with its description, references and YAML. |
| `POST /api/templates/passive` | Body `{"ids": [...], "enabled": bool}` or `{"filter": {...list query...}, "enabled": bool}`. Ids that cannot run passively are ignored and counted. |

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
```

`download` and `import` wait until the release is installed and print the final
status. `passive set-enabled` takes exactly one of ids, a filter or `--all`.

## Not yet

Active scans — sending a template's requests to targets you choose, on demand,
with scope checks, rate limits and acknowledgment — are planned and not part of
this release. Until then no template sends anything.
