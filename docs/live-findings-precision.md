# Live Findings: bounded wording and matching corrections

## Cache context is an indicator, not proof of authentication

The cache findings now use these titles:

- `Missing Cache-Control with session or authorization indicators`
- `Cache-Control: public with session or authorization indicators`

Both details describe the observed Authorization header or session-like cookie
and explicitly say that it does not establish an authenticated user. A first,
anonymous response can issue a session cookie; a supplied Authorization header
can be rejected. The details therefore describe caching of *any* sensitive
content conditionally rather than asserting that the response is authenticated.

This is a wording-only correction. The existing request/response cookie and
Authorization checks, empty/analytics/CSRF/deleted-cookie exclusions, private
and no-store restrictions, static/script exclusions, severity, category,
evidence and navigation are unchanged. No cookie or credential value is added
to the evidence. The missing-header finding keeps empty evidence and response
headers navigation; the public-cache finding keeps its Cache-Control evidence
and header location.

### Existing saved Findings and explicit Rescan

An exact title is part of the existing deduplication key. Old and new titles
therefore remain distinct, and a saved legacy finding can coexist with a new
live finding before an explicit successful Rescan. Restoring a session does
not migrate or rescan these findings. Both titles retain the same endpoint
scope because they still contain `Cache-Control`. No dedup alias, automatic
migration, or persistence change is introduced.

The supported Findings Rescan action replaces findings using retained records
and current rules, then persists the result. Check its outcome: an unreadable
record count, conflict or failure is not a successful refresh of all history.
The existing persist-failure rollback remains unchanged. Installing this patch
does not refresh old findings, and this change does not automatically Rescan
user data.

### Coverage and validation status

Synthetic example.com tests pair newly issued anonymous JSESSIONID, request
session cookies and explicit Authorization with both missing and public cache
policies. They assert exact titles/details and unchanged severities, evidence
and locations. Negative controls cover absent indicators, empty Authorization,
CSRF-only and analytics cookies, and deleted response sessions. Existing tests
continue covering private/no-store restrictions and script/static exclusions.
A store test documents old/new title coexistence and unchanged endpoint scope.
The existing supported API Rescan replacement test also seeds a legacy cache
finding and checks the new title/detail in memory and reloaded persisted state.

Native compilation, Rust tests and runtime validation are **UNRUN** for this
candidate pending the coordinated native gate. Direct formatting and whitespace
checks do not substitute for that gate.

## Replicate matches must be complete within ASCII opaque identifiers

A token-shaped substring can appear inside a larger URL-safe opaque identifier,
including a challenge payload. The Replicate rule now rejects a candidate when
its immediately preceding or following byte is ASCII alphanumeric, underscore
or hyphen. This guard is specific to Replicate and does not suppress challenge
pages, 403 responses, other providers, or API-key findings as a class.

The existing supported pattern is unchanged: `r8_` plus exactly 38 ASCII
alphanumeric characters. This correction makes no claim about other vendor
formats and adds none. It inspects adjacent bytes without consuming delimiters
or slicing partial UTF-8 characters. Accepted evidence remains the exact
41-character match; severity, generic filters and evidence-based deduplication
are unchanged. A rejected candidate does not end the search for later matches.

Quotes, whitespace/newlines, JSON delimiters, parentheses, semicolons, and URL
query equals/ampersand separators remain accepted. Authorization-style text is
tested as response body content; this change does not add actual header
scanning to the body-only rule.

### Boundary tradeoffs and accepted-match navigation

Unicode letters, CJK prose, combining marks and Unicode punctuation remain
accepted next to a supported token. A Unicode word-boundary rule would reject
some of those display cases and is deliberately not used. A token-shaped
substring inside a true Unicode identifier can therefore still be reported;
Unicode source-language identifier parsing is outside this correction.

ASCII hyphen is conservatively treated as URL-safe identifier continuation.
A genuine token touching ordinary hyphen punctuation can consequently be
missed. This is the explicit precision tradeoff, not a claim that every hyphen
is part of an identifier.

For Replicate only, navigation uses the accepted regex match's byte offset and
the existing response header/body line calculation. An identical value inside
an earlier rejected opaque identifier or a response header no longer diverts
navigation away from the accepted body match. Generic location inference for
all other credential signatures and evidence-based deduplication are unchanged.

### Replicate coverage and validation status

Six scanner tests cover the observed 5-byte-prefix/4-byte-suffix opaque shape,
one-sided and two-sided ASCII continuation, JSON, quoting, literal newlines,
punctuation, URL parameters, body text shaped like an Authorization header,
Unicode adjacency, unchanged token prefix/alphabet/length, and later accepted
matches after rejected candidates. Accepted cases assert exact evidence and
High severity. Paired continuation cases include identical and distinct values,
Unicode preceding both matches, and an earlier repeated value in a header;
they assert the accepted body line and unchanged exact evidence/dedup scope.
A 403 fixture retains a separate GitHub provider finding to guard against
blanket suppression.
All values are synthetic and all test URLs use example.com. No suspected live
value is retained and no credential is validated.

Native compilation, Rust tests and runtime validation for this correction are
also **UNRUN** pending the coordinated native gate. No Cargo/build activity or
network calls were made by this implementation pass.
