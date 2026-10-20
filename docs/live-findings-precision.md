# Live Findings: wording and matching corrections

## Cache findings name indicators, not authentication

The cache findings are now titled:

- `Missing Cache-Control with session or authorization indicators`
- `Cache-Control: public with session or authorization indicators`

An Authorization header or session-like cookie does not prove an authenticated
user: an anonymous first response can set a session cookie, and a sent
Authorization header can be rejected. The details now name the observed
indicator, say it does not prove authentication, and describe caching of
sensitive content as a conditional risk.

Only the wording changed. Detection, exclusions, severity, category, evidence
and navigation are the same, and no cookie or credential value is added to the
evidence.

The title is part of the deduplication key, so a saved finding with the old
title can sit next to a new one until a successful **Rescan**. Restoring a
session does not migrate or rescan findings. Both titles keep the same endpoint
scope because both contain `Cache-Control`. Rescan rebuilds findings from
retained records with the current rules and saves the result; if it reports
unreadable records, a conflict or a failure, history was not fully refreshed.

## Replicate tokens must stand alone

A token-shaped substring can sit inside a longer URL-safe opaque identifier,
such as a challenge payload. The Replicate rule (`r8_` plus exactly 38 ASCII
letters and digits) now rejects a match when the byte right before or after it
is an ASCII letter, digit, underscore or hyphen, and keeps searching for later
matches. The guard applies to Replicate only; challenge pages, 403 responses
and other providers are unaffected. Evidence is still the exact 41-character
match, and severity and deduplication are unchanged.

- Quotes, whitespace, newlines, JSON delimiters, parentheses, semicolons, `=`
  and `&` still end a token.
- Unicode letters, CJK text, combining marks and Unicode punctuation also end a
  token, so a token inside a Unicode identifier can still be reported.
- A hyphen counts as part of an identifier, so a real token touching a hyphen
  is missed. This is the chosen tradeoff.
- Navigation now uses the accepted match's byte offset, so the same value in a
  rejected identifier or a response header no longer misdirects it. Other
  credential signatures keep the generic location lookup.

The rule still scans response bodies only.
