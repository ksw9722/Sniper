# Capture Findings precision review

These are precision fixes to the built-in passive scanner rules in
`src/scanner.rs`. All ten rule families (`jwt`, `header`, `cookie`,
`disclosure`, `cors`, `server`, `error`, `misconfig`, `info`, `auth`) stay
enabled by default with the same severities, and deduplication and **Rescan**
work as before. Custom regex rules and Nuclei templates are not covered here.

## Changes

- **CSP effective policy.** The old check accepted `unsafe-eval` in
  `script-src-elem` and `script-src-attr`, counted inline sources that a
  fallback overrode, merged repeated directives and read only the first enforced
  header. It now follows CSP3: the first duplicate directive wins, element and
  attribute fallbacks are independent, and every enforced policy counts. It
  models header-delivered script sources only, not meta policies, sandbox,
  nonce quality or older browsers.
- **Cache-Control syntax.** `contains("public")` matched
  `x-public-cache=enabled` or a quoted field name, and `x-no-store=disabled`
  could hide a real `public`.
  The rule now parses comma-separated members, including quoted strings, across
  all field lines, and honors real `no-store` and unqualified `private`.
- **Authentication context.** The cache rule treated recognized analytics
  cookies as authentication, though the cookie rule already ignored them, and
  empty or literal-null values counted too. Both rules now share one predicate
  that still counts real session and JWT cookies.
- **Challenge schemes.** `contains("basic")` read a Digest realm or a Bearer
  error description as a Basic challenge. The rule now reads the scheme outside
  quoted values, including combined and repeated header lines.
- **HTML comments.** Substrings such as `tokenization`, `secretary`, `product`
  and `author` passed the keyword and context checks. Keywords now need word
  boundaries and an HTML response. Credential assignments, internal paths and
  security notes still match; JavaScript and JSON strings are not comments.
- **CORS header values.** `Access-Control-Allow-Credentials: True` counted as
  credentialed because the compare ignored case, while Fetch requires exact
  `true`. The rule now requires exact `true` and `null`, trimming only spaces
  and tabs, and ignores responses with duplicate `Access-Control-Allow-Origin`
  or `Access-Control-Allow-Credentials` fields, which Fetch combines and so
  rejects. Duplicate request `Origin` fields do not count as one reflected
  origin. A non-null origin must equal its serialized form, so paths, userinfo,
  default ports, case changes and lists do not match. Same-site filtering, the
  allowlist warning and the wildcard exclusion are unchanged, and `null` with
  `true` stays a Medium policy observation even without a matching request
  `Origin`.

The new parsing is linear in header size and adds no regex or recursion. Comment
matching keeps its 500-character window, and capture limits are unchanged.

New captures use these rules once the app is rebuilt. Stored findings are not
rewritten; run **Rescan** to re-evaluate retained history.

## Open items

- A returned email, OpenAPI schema, GraphQL introspection response, management
  page or source map can be deliberate. Do not disable these rules wholesale
  without the use case and request context.
- A source-map URL does not show that the map is downloadable, and a sensitive
  path mention does not show that the file is served. A future rule could
  separate references from returned content without fetching anything.
- Missing `exp` does not prove the server has no expiry; the
  [follow-up pass](passive-findings-precision-next-pass.md) changed that wording.
- Some comment matches stay low-severity leads, such as a literal `admin` note.
- Blank debug headers, intentional diagnostics, file-path references,
  vendor-shaped documentation tokens and quoted error examples need their own
  fixtures.

## References

- RFC 9110 [authentication challenges](https://www.rfc-editor.org/rfc/rfc9110.html#section-11.3) and [quoted strings](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.4)
- RFC 9111 [Cache-Control](https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2), [`private`](https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.2.7) and [`no-store`](https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.2.5)
- CSP3 [policy parsing](https://www.w3.org/TR/CSP3/#parse-serialized-policy), [fallback lists](https://www.w3.org/TR/CSP3/#directive-fallback-list), [script-src](https://www.w3.org/TR/CSP3/#directive-script-src), [multiple policies](https://www.w3.org/TR/CSP3/#multiple-policies) and [inline behavior](https://www.w3.org/TR/CSP3/#allow-all-inline)
- Fetch [CORS and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials) and [CORS check](https://fetch.spec.whatwg.org/#cors-check); URL [origin](https://url.spec.whatwg.org/#concept-url-origin)
