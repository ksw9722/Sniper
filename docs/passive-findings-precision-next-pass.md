# Capture Findings: bounded follow-up precision pass

Base: `ed528dc649e543bccefe7610a97dc9112482ff7d`, reconciled with upstream
`0196dfe4da3d8b17fa12c6eb761a6cfbfd264def` and the earlier built-in precision
patches. This is a separate candidate, not a modification of the frozen native
qualification worktree. It changes built-in scanner predicates and explanatory
text only. Nuclei templates, user configuration, persistence, retention,
workspace saving, rule toggles, and existing Rescan behavior are unchanged.

All examples are synthetic. The review does not identify the user's particular
false positives or measure real-traffic frequency. No traffic, live target,
template download, template enablement, or active scan is used.

## Case decisions and paired coverage

| Case | Earlier behavior | Candidate and preserved controls | Compatibility / false-negative boundary |
| --- | --- | --- | --- |
| Oracle error evidence | Any `ora-` substring in a 4xx/5xx body, including `flora-guide`, established an Oracle error. The excerpt always started at the first prefix. | Require a complete `ORA-` plus five ASCII digits with word boundaries. Preserve lowercase and canonical ORA-00942/01017/00600/06502/06512 forms, including a standalone complete code. Find the qualifying occurrence and use its original-case byte offset for evidence and exact response-body navigation. | Deliberately does not infer a complete code from bare or truncated prefixes, omitted leading zeros such as ORA-942, non-five-digit forms, or identifier-embedded tokens. A copied full code in a returned error document may still be an observation rather than a real database failure. |
| ColdFusion attribution | The generic phrase `Error occurred while processing request` alone produced a ColdFusion finding. | Require the phrase plus a nearby implementation-package or default diagnostic-resource marker. Preserve Medium severity and title when corroborated. A later corroborated occurrence is not hidden by an earlier generic heading; bounded evidence explicitly includes both heading and vendor marker. Token boundaries are checked against the full body, so cropping cannot invent one; navigation uses the qualifying marker offset rather than a second substring search. | The nominal 512-byte window on each side (rounded to UTF-8 boundaries) is a precision/performance heuristic, not an Adobe layout guarantee. Minimized, localized, customized, or widely separated diagnostics can be missed. Generic error text alone no longer identifies a vendor; other independent error rules still run. |
| HSTS on IP literals | A successful/redirecting HTTPS document addressed by IP could receive a missing-HSTS finding. | Suppress only this HSTS check for parsed IPv4/IPv6 literals, including bracketed IPv6 and valid explicit ports in stored authorities. Keep domain and localhost checks, redirect handling, and every other security-header finding. | No hostname resolution or broad local/private-host exemption. Stored host fields may be a bare address or retain an authority port, depending on the capture path; both parsed shapes are handled. Hostname authorities and malformed ports/brackets are not guessed to be IP addresses. |
| JWT lifetime detail | Missing `exp` was described as proof of indefinite reuse. | Keep the finding identity, Medium severity, and predicate; explain that server-side expiry, revocation, or other validation may limit lifetime. | This is more accurate explanatory text, not proof of server validation or a new severity policy. |
| CORS access detail | Headers were described as proof another site read the response with credentials. | Keep the identity, Low severity, exact-value checks, and allowlist caveat; explicitly distinguish permitted headers from actual credentials, sensitive response content, and a completed browser read. | Passive headers still cannot distinguish an intended allowlist from unrestricted reflection or establish every browser prerequisite. |

The ColdFusion corroborators are a small fixed list: implementation namespaces
`coldfusion.runtime.`, `coldfusion.tagext.`, `coldfusion.filter.`,
`coldfusion.sql.`, `coldfusion.compiler.`, and diagnostic-resource phrases
`check the coldfusion documentation` / `enable robust exception information`.
They require a preceding token boundary; a mere product-name mention is
insufficient. The selection is an implementation heuristic informed by Adobe's
error-format, stack-trace, and debugging documentation, not a universal error
page grammar.

## Primary references

- [Oracle SQLJ error-message format](https://docs.oracle.com/cd/F49540_01/DOC/java.815/a64684/keyprog5.htm) describes the ORA prefix followed by a five-digit error code. [Current ORA-00942 documentation](https://docs.oracle.com/en/error-help/db/ora-00942/) supplies a canonical retained example. Integer-returning APIs and shortened log formatting are not interchangeable with this displayed-token check.
- [Adobe standard error format](https://guides.adobe.com/coldfusion/en/docs/develop-coldfusion-applications/__references__/error-messages-and-the-standard-error-format.html) distinguishes descriptions, diagnostics, locations, resources, environment information, and traces, and notes that not every section appears. [Debugging options](https://guides.adobe.com/coldfusion/en/docs/develop-coldfusion-applications/configuring-debugging-in-the-coldfusion-administra.html) describe robust exception information. [Exception types](https://guides.adobe.com/coldfusion/en/docs/develop-coldfusion-applications/understanding-errors.html) identifies a ColdFusion-specific exception class.
- [RFC 6797 §8.1.1](https://www.rfc-editor.org/rfc/rfc6797.html#section-8.1.1) says IP-literal/IPv4address hosts must not become known HSTS hosts. The exception here is confined to that rule.
- [RFC 7519 §4.1.4](https://www.rfc-editor.org/rfc/rfc7519.html#section-4.1.4) defines the expiration claim and makes its use optional; absence alone does not establish the server's effective lifetime policy.
- [Fetch CORS check](https://fetch.spec.whatwg.org/#cors-check) evaluates header values, origin, and credentials mode. Observing response headers is not evidence that authenticated sensitive data was actually read.

## Validation and boundedness

Six new Rust scanner test functions cover the paired controls above, including
an early benign prefix followed by a later genuine code and multibyte Korean
text before the match. Duplicate lookalikes/markers also verify exact navigation
to the qualifying occurrence. Tests assert preserved finding titles/severities,
4xx/5xx filtering, other header findings on IP hosts, and domain redirects and port-bearing capture-shaped authorities.

**The application crate and these six native scanner tests have not been
compiled or executed for this candidate.** They require the next coordinated
native qualification gate; no Cargo/shared-target use occurred here.

A separate standalone Rust harness compiled the exact newly added std-only
helpers extracted from the candidate, together with the unchanged UTF-8-safe
evidence-window helpers. It passed **68 paired/boundary checks**, including
600,009-byte repeated Oracle lookalikes with a final genuine code and
4,359,389 bytes of repeated generic headings separated from a vendor marker.
The observed run took approximately 14 ms on this executor. This is a synthetic
smoke control, not a throughput benchmark, application integration test, or
real-traffic claim. Formatting and diff-whitespace checks also passed. The existing Findings UI,
Rescan lifecycle, and scanner-configuration suite passed **277 in-memory Node
tests**; these do not compile or exercise the changed Rust predicates.

Matching remains iterative over retained previews. Oracle uses a fixed-size
numeric check per candidate. ColdFusion scans a fixed marker list in a bounded
local window per heading. There is no new regex compilation, recursive parser,
network access, or capture/body/header size-setting change. Existing error-rule
ordering and the one-error-finding-per-response policy remain intact.

After an integrated rebuilt application is available, new captures use these
rules. Existing stored Findings are reevaluated only through the existing
user-triggered Rescan of retained local history; this candidate does not
silently delete, rewrite, or rescan user data.

Empty debug-header policy and source-map lexing remain analysis-only and are
not changed in this pass.
