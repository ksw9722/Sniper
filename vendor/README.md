# reqwest raw request-target patch

Sniper vendors the crates.io `reqwest` **0.12.28** source package because its
public URL-based API normalizes literal and percent-encoded dot segments before
sending them. A testing proxy must preserve the accepted wire path and query.
Changing escaping is not an equivalent fix.

## Provenance

- Registry package: https://crates.io/crates/reqwest/0.12.28
- Upstream repository: https://github.com/seanmonstar/reqwest
- Original `.crate` SHA-256:
  `eddd3ca559203180a307f12d114c268abf583f59b03cb906fd0b3ff8646c1147`
- The package's `.cargo_vcs_info.json` records revision
  `ef2768a823b28bf00e23e218e034be035b08d770` and `dirty: true`; the published
  registry archive and its checksum are the authoritative comparison source.
- Original dual MIT / Apache-2.0 licenses, README, manifests and source are
  retained. Cargo's local registry-install marker/checksum files are omitted.

## Local changes

Only `src/async_impl/request.rs` and `src/async_impl/client.rs` differ from the
published source. `RequestBuilder::raw_path_and_query` accepts a typed
`http::uri::PathAndQuery` whose path must start with `/`. It stores a private,
cloneable extension together with the URL at configuration time. At execution,
the URL must still match; changing URL/query afterwards fails before dialing.
The hook replaces only the validated URI's path/query, after existing URL
scheme and authority validation and before reqwest's existing middleware.

Retries clone the raw URI. Redirects construct their own URI and do not reuse
the override. Sniper disables redirects for forwarding and Replay. TLS,
certificate validation, proxy authentication/bypass, DNS overrides, HTTP/2,
connection pooling, timeout and response handling are unchanged. Response URL
metadata remains normalized; Sniper's history retains its own raw target.
Existing path validation still rejects controls, whitespace and fragments
before constructing the URI. `//` is a valid origin path and cannot change the
URI authority here. CONNECT handling is separate; unsupported `*` Replay
requests remain rejected before reaching the hook.

Sniper opts in only in the two shared streaming/buffered HTTP send paths.
Unrelated reqwest clients keep upstream behavior.

## Updating

1. Check upstream security advisories and whether an equivalent supported API
   removes the need for this patch. Never remove the override without wire tests.
2. Verify the new registry archive checksum, retain its licenses, and replace the
   source from that archive. Reapply only these two documented changes (or adopt
   the upstream API). Do not modify URL internals or encode the user's target.
3. Update the exact version in the root manifest and lockfile. Avoid unrelated
   dependency changes. Update this provenance and review the complete upstream
   source diff, including redirect/retry ordering.
4. Run `cargo test --release --test raw_request_target_transport`, the normal
   release suite, and `python tests/raw_request_target_smoke.py target/release`
   against rebuilt binaries. The latter covers running-instance HTTP, HTTPS
   MITM, Replay, capture/wire equality, request bodies/headers, redirects and
   authenticated HTTP/SOCKS chains with bypass. No system trust is changed.
