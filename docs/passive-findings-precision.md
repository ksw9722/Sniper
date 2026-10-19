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
claim about any running session's configuration.

| Family | Checks | Existing precision guards | Review result |
| --- | --- | --- | --- |
| `jwt` | `alg:none`, missing `exp`; bearer/cookies/response tokens | JOSE-header parsing, JSON payload, body token context | Keep. A passive token observation does not prove server acceptance or indefinite validity. |
| `header` | HSTS, CSP, nosniff, frame restrictions | Document/frame context; success HTML; valid header values; CSP frame-ancestors alternative | Keep. These are hardening observations; public documents may intentionally permit embedding. |
| `cookie` | HttpOnly, Secure, SameSite | Auth-like names/JWT values; deletion filtering; known analytics exclusions | Share the analytics/empty-value predicate with cache checks; preserve actual session cookies. |
| `disclosure` | 64 token/secret/path/card signatures plus private-address detection | Binary/base64 filtering, placeholders, credential-bearing DB URLs, Luhn/context checks | Keep in this pass. File-path references and vendor-shaped documentation tokens merit separate contextual fixtures. |
| `cors` | Credentialed null origin; another site's reflected/allowed origin | Wildcard case excluded; same-site heuristic; low-confidence cross-site wording | Keep. A legitimate allowlist cannot be distinguished from unrestricted reflection using one captured exchange. |
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
