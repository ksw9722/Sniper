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
