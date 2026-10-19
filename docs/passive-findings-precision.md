# Capture Findings precision review

Baseline: `75ceec7a8756f351ff0551acb440600bb89be31f` (Sniper 0.3.2).

This is a code review and synthetic-fixture exercise. It does not measure real
traffic, demonstrate exploitability, or identify the user's particular false
positives. No captured user data, credentials, external targets, or active
scanning is used. Rules and severity levels remain enabled/unchanged. Custom
regex rules remain under the user's existing configuration.

## Inventory

All ten built-in families default to enabled in `BUILTIN_RULES` and
`default_rule_toggles` in `src/scanner.rs`. This is the repository default, not a
claim about any running session's configuration. This inventory covers the
built-in scanner on this baseline only. Later upstream changes add a separate
passive Nuclei Templates engine/category and Rescan integration; those templates
and the user's enabled-template configuration are not covered by this review.
No templates were downloaded, enabled, or executed here.

| Family | Checks | Existing precision guards | Review result |
| --- | --- | --- | --- |
| `jwt` | `alg:none`, missing `exp`; bearer/cookies/response tokens | JOSE-header parsing, JSON payload, body token context | Keep. A passive token observation does not prove server acceptance or indefinite validity. |
| `header` | HSTS, CSP, nosniff, frame restrictions | Document/frame context; success HTML; valid header values; CSP frame-ancestors alternative | Keep. These are hardening observations; public documents may intentionally permit embedding. |
| `cookie` | HttpOnly, Secure, SameSite | Auth-like names/JWT values; deletion filtering; known analytics exclusions | Share the analytics/empty-value predicate with cache checks; preserve actual session cookies. |
| `disclosure` | 64 token/secret/path/card signatures plus private-address detection | Binary/base64 filtering, placeholders, credential-bearing DB URLs, Luhn/context checks | Keep in this pass. File-path references and vendor-shaped documentation tokens merit separate contextual fixtures. |
| `cors` | Credentialed null origin; another site's reflected/allowed origin | Wildcard case excluded; same-site heuristic; low-confidence cross-site wording | Enforce exact Fetch values and serialized-origin syntax in a separate patch. A legitimate allowlist still cannot be distinguished from unrestricted reflection using one captured exchange. |
| `server` | Version banners and debug/backend headers | Dotted versions; edge/gateway/platform exclusions | Keep. Blank debug headers and intentional diagnostics merit future controls. |
| `error` | 58 database/language/framework patterns | 4xx/5xx only; binary exclusion; generic-pattern/context suppression | Keep. Error documentation/quoted examples and occurrence-local context need a separate review. |
| `misconfig` | CSP unsafe execution; authenticated cache policies | Nonce/hash and directive context; static-resource exclusions | Correct directive syntax, cookie auth context, and separately review CSP enforcement semantics. |
| `info` | Directory listing, maps, introspection, comments, emails, commit/spec/WSDL shapes, mapped management/config exposures | Structured-document checks, MIME checks, public/example email exclusions, exposure-body limit | Require actual HTML context and whole comment keywords. Other intentional disclosures need examples rather than blanket suppression. |
| `auth` | URL tokens/userinfo, HTTP Basic credentials/challenges, possible redirects | Exact token query names, sensitive values, external-origin/location equality | Parse challenge schemes instead of searching parameter text for `basic`. Preserve actual Basic challenges and existing redirects. |

The signature counts describe code entries, not incidents or unique findings.
Existing issue-scoped deduplication and `Rescan` behavior are deliberately
unchanged. Persistence, retention, workspace save, and scanner configuration
state are outside this change.

## Prioritized deterministic cases

1. **CSP effective policy, separate patch.** The old functions accept
   `unsafe-eval` in `script-src-elem`/`script-src-attr`, include overridden
   inline fallback sources, overwrite or combine repeated directives, and
   inspect only the first enforced header. These can disagree with browser
   enforcement. Cover restrictive/unsafe pairs, both duplicate orders,
   independent element/attribute fallbacks, and multiple enforced policies.
2. **Cache directive syntax.** `contains("public")` treats
   `x-public-cache=enabled` or a quoted field name as the `public` directive.
   Conversely, `x-no-store=disabled` must not suppress a real `public` finding.
   Parse comma members with quoted-string handling, inspect all field lines,
   and honor actual `no-store`/unqualified `private` restrictions.
3. **Authentication context.** The cookie rule already ignores recognized
   analytics cookies, while the cache rule previously treated the same
   session-like cookie name as authentication. Empty and literal-null values
   also established auth context. Use one shared predicate, retain ordinary
   session/JWT cookies, and test a mixed analytics plus real-session request.
4. **Challenge parameters versus schemes.** `contains("basic")` mistakes a
   Digest realm or Bearer error description for a Basic challenge. Recognize
   the scheme outside quoted values, including combined/repeated field lines.
5. **HTML comment substrings/context.** `tokenization`, `secretary`, and
   unrelated `product`/`author` words can satisfy the old keyword/context
   substring checks. Require keyword boundaries and an HTML response; retain
   credential assignments, internal paths, and security-related development
   notes. JavaScript/JSON string contents are not HTML comment nodes.

## Protocol references

- [RFC 9110, authentication challenges](https://www.rfc-editor.org/rfc/rfc9110.html#section-11.3): a challenge begins with an authentication scheme; parameters are distinct. [Quoted strings](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.4) can contain escaped characters and commas.
- [RFC 9111, Cache-Control](https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2): directive names and arguments have defined syntax. [Private](https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.2.7) without an argument prevents shared storage; field-qualified private only restricts named headers. [No-store](https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.2.5) prevents storage.
- [CSP3 parsing](https://www.w3.org/TR/CSP3/#parse-serialized-policy): first duplicate directive wins. [Fallback lists](https://www.w3.org/TR/CSP3/#directive-fallback-list), [script-src](https://www.w3.org/TR/CSP3/#directive-script-src), and [multiple policies](https://www.w3.org/TR/CSP3/#multiple-policies) define effective restrictions. [Inline behavior](https://www.w3.org/TR/CSP3/#allow-all-inline) handles nonce/hash and strict-dynamic overrides.

## Remaining judgment calls

- A returned email, OpenAPI schema, GraphQL introspection response, management
  page, or source-map reference may be deliberate. Presence is evidence of
  disclosure, not evidence of unauthorized access. Do not disable these across
  the board without the use case and request context.
- A source-map URL does not show whether the map is downloadable; a sensitive
  pathname mention does not establish that the file is served. A future rule
  can separate references from returned content without fetching a target.
- A credentialed cross-site CORS response can be an intentional allowlist.
  Passive confidence should distinguish observation from verified arbitrary
  origin acceptance. The current finding already states this limitation.
- Missing `exp` does not prove that the application has no server-side expiry
  or revocation. Future wording/context work should avoid that implication.
- Some comment matches will remain low-severity review leads, e.g. a literal
  `admin` annotation. Exact keyword boundaries are a bounded precision fix,
  not semantic understanding of every developer comment.
- Primary standards describe browser/server behavior, but synthetic unit tests
  do not qualify all browser versions, intermediary quirks, malformed headers,
  or real-world false-positive rates.

### Separate CORS header-value patch (source-reviewed; not compiled)

The earlier code treated `Access-Control-Allow-Credentials: True` as
credentialed access through `eq_ignore_ascii_case`. The [Fetch credentials
matrix](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials) explicitly
requires lowercase `true`, and the [CORS check](https://fetch.spec.whatwg.org/#cors-check)
compares the origin against its serialization. The separate patch:

- Requires exact `true` and exact opaque-origin `null`, trimming only HTTP
  SP/HTAB whitespace. Unicode whitespace and differently cased values do not
  establish browser permission.
- Rejects duplicate ACAO/ACAC response fields, including empty duplicates;
  Fetch combines these fields and they cannot pass the singleton comparisons.
  Duplicate request Origin fields likewise do not establish one reflected
  origin. Unrelated repeated headers do not suppress a finding.
- Requires non-null origin values to equal their canonical URL-origin
  serialization. Paths, userinfo, default ports, case-normalized values, and
  origin lists cannot match a browser's serialized Origin. [URL origin rules](https://url.spec.whatwg.org/#concept-url-origin)
  retain canonical FTP/WS/WSS tuple origins as well as HTTP(S); there is no
  additional HTTP(S)-only allowlist.
- Retains existing severity, same-site filtering, intentional-allowlist warning,
  wildcard exclusion, and exact null-origin policy observation. Null plus true
  remains a Medium policy observation even without a matching request Origin;
  it is not proof the captured transaction passed CORS.

Three new Rust test functions pair malformed and valid values, duplicate and
unrelated fields, null/credentialed-cross-site positives, whitespace cases,
canonical IPv4/IPv6/punycode/trailing-dot/port-zero origins, and standard tuple
schemes. Their source is reviewed but **has not been compiled or executed**.
The parent integration gate must run them alongside the existing CORS and full
project regressions. The native results below cover the two earlier frozen
patches only, not this CORS follow-up. No real traffic statistics are inferred.

Origin parsing uses the existing `url` dependency, with a fixed number of
header scans and no new regex compilation or recursive parser. Existing
capture limits are unchanged. Nonstandard browser-extension origin semantics
are outside this standards-level parser check; this patch does not emulate
browser extensions or prove allowlist intent.

### CSP scope

The CSP patch models modern CSP3 header-delivered script source restrictions.
It does not parse HTML meta policies or sandbox restrictions, evaluate nonce
quality, or emulate historical browser versions. It reports source-policy
observations, not a demonstration that a script can execute in every document
context. No browser execution or target probing is part of qualification.

## Applying the rule changes

New captures use the updated built-in rules after rebuilding/running the
updated application. Findings already stored in a session are not silently
rewritten by this patch. The existing user-triggered **Rescan** action
reevaluates retained local history using current rules. Use that action when
reviewing whether old false positives disappear; absence of an automatic
change to old rows does not mean the new predicates failed. No automatic clear,
rescan, template change, or user-data mutation is introduced here.

## Qualification of the first two built-in patches (before CORS)

- Final focused native command: `cargo test --offline --release --lib scanner::tests -- --test-threads=1`.
- Result: **199 passed, 0 failed**. Includes 16 added tests, each using synthetic
  records, with paired benign/suspicious controls and existing-rule regressions.
- The reviewed source SHA-256 is `7577f3749c8b10267c71550fbe224dd36950c94e586f674290d490becfb4127c`.
- The qualified test-binary SHA-256 is `38bac6b3ab766c8d340ece0c078e521352e158a4291e8d9a67d5eb310b114c98`.
- Findings UI, Rescan lifecycle, and scanner configuration regressions:
  **277 in-memory Node tests passed**. No live runtime or socket was used.
- Separate independent reviews approved the simple filters and CSP semantics;
  review suggestions became additional controls before the final native run.
- Formatting and diff whitespace checks passed. This is focused source/test
  qualification, not a full-project suite, browser execution, package, release,
  or validation of the later upstream Nuclei engine.

### Boundedness check

New header/CSP parsing is iterative and linear in retained header text/source
lists, with no recursive parsing or newly added regex compilation. Comment
matching retains the existing 500-character regex window and captured body
preview boundary; token-boundary matching adds only a fixed set of aliases.
No capture/body/header size setting is changed. This does not add a new global
header-byte limit.

A standalone Rust control compiled the exact new parser helpers and checked a
524,294-byte header (16,385 quoted members), 688,147 bytes of repeated CSP
directives, and 16,384 enforced policies. All assertions passed in approximately
8 ms on this executor. This is a bounded synthetic smoke check, not a throughput
benchmark or a claim about real traffic latency.
