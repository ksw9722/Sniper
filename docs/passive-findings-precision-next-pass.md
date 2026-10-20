# Capture Findings: follow-up precision pass

This pass changes more built-in scanner predicates and finding text. Nuclei
templates, user configuration, rule toggles, persistence and **Rescan** work as
before. Stored findings change only when you run Rescan.

## Changes

- **Oracle errors.** Any `ora-` substring in a 4xx/5xx body, such as
  `flora-guide`, counted as an Oracle error, and the excerpt always started at
  the first one. The rule now needs a complete `ORA-` code with five ASCII
  digits and word boundaries, matched case-insensitively, and points evidence
  and navigation at that occurrence. Bare or truncated prefixes, codes missing
  leading zeros (`ORA-942`), other digit counts and codes inside identifiers no
  longer match.
- **ColdFusion attribution.** The generic phrase `Error occurred while
  processing request` alone produced a ColdFusion finding. It now also needs a
  marker within about 512 bytes on either side, after a token boundary: one of
  the namespaces `coldfusion.runtime.`, `coldfusion.tagext.`,
  `coldfusion.filter.`, `coldfusion.sql.` or `coldfusion.compiler.`, or the
  phrases `check the coldfusion documentation` or `enable robust exception
  information`. A product-name mention is not enough. Title and Medium severity
  are unchanged. Evidence shows both the phrase and the marker, navigation uses
  the marker, and an earlier uncorroborated heading no longer hides a later
  corroborated one. Minimized, localized, customized or widely separated
  diagnostics can now be missed; the other error rules still run.
- **HSTS on IP addresses.** HTTPS documents addressed by an IPv4 or IPv6
  literal, including bracketed IPv6 and explicit ports, no longer get a
  missing-HSTS finding, because RFC 6797 says IP hosts never become HSTS hosts.
  Domains and `localhost` are still checked, and other header findings are
  unchanged. There is no DNS lookup and no exemption for private hosts.
- **JWT lifetime wording.** A missing `exp` no longer reads as proof of
  indefinite reuse; the detail notes that server-side expiry or revocation may
  limit the token. The finding, its Medium severity and the check are
  unchanged.
- **CORS wording.** The finding no longer says another site read the response
  with credentials. It separates the permitted headers from credentials
  that were sent, sensitive content and a completed browser read. The finding,
  its Low severity and the exact-value checks are unchanged.

Matching stays iterative over retained previews, with no new regex, recursion,
network access or size-limit change. Error-rule order and one error finding per
response are unchanged. Empty debug headers and source-map lexing are not
changed in this pass.

## References

- Oracle [SQLJ error-message format](https://docs.oracle.com/cd/F49540_01/DOC/java.815/a64684/keyprog5.htm) and [ORA-00942](https://docs.oracle.com/en/error-help/db/ora-00942/)
- Adobe ColdFusion [standard error format](https://guides.adobe.com/coldfusion/en/docs/develop-coldfusion-applications/__references__/error-messages-and-the-standard-error-format.html), [debugging options](https://guides.adobe.com/coldfusion/en/docs/develop-coldfusion-applications/configuring-debugging-in-the-coldfusion-administra.html) and [exception types](https://guides.adobe.com/coldfusion/en/docs/develop-coldfusion-applications/understanding-errors.html)
- [RFC 6797 §8.1.1](https://www.rfc-editor.org/rfc/rfc6797.html#section-8.1.1): IP literals are not HSTS hosts
- [RFC 7519 §4.1.4](https://www.rfc-editor.org/rfc/rfc7519.html#section-4.1.4): `exp` is optional
- [Fetch CORS check](https://fetch.spec.whatwg.org/#cors-check)
