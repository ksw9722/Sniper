# CLI and saved-workspace maintenance (unreleased)

This is an overview of maintenance work on `release/0.3.1`, not a release
announcement. The package version remains `0.3.0` until release preparation.

## Predictable CLI contracts

- Discovery, examples, and schemas describe the corrected saved-data and local
  skill-maintenance inputs and result shapes. Existing accepted UUID spellings
  remain supported where documented.
- Saved-data response checks bind results to the requested session, selection,
  page limit, and continuation boundaries. Continue using explicit session IDs
  and pass returned continuations unchanged.
- Parse and local input-file failures have structured errors. Saved-data input
  errors distinguish unreadable sources, invalid JSON, and schema failures without
  echoing paths or contents, including stderr fallback when output fails.
  Non-Unicode argument bytes are suppressed; ordinary non-saved argument
  diagnostics and file paths may still appear. After API discovery, failed generic
  API reads use the actual HTTP status without echoing server error bodies.
  Some failures consequently have
  different error codes or exit codes; branch on the documented contract, not
  message fragments.
- Legacy session create, rename, switch, and delete send the mutation once without
  following redirects or retrying it. Rename and switch also require the returned
  session UUID to match the requested UUID; equivalent spellings remain accepted.
  A mismatched acknowledgement leaves the mutation outcome unknown. Unsuccessful
  HTTP responses use the received status without reading or echoing error bodies;
  these failures report an unknown outcome and remain non-retryable.
- Rule enable/disable and saved-tab label acknowledgements must contain their
  promised non-null fields before successful output is reported. Malformed,
  incomplete, or mismatched acknowledgements report an unknown write outcome;
  they are not automatically retried. Additive fields and equivalent spellings
  of UUID-typed session and rule IDs remain accepted; tab IDs stay exact strings.
- Applied saved rename receipts bind the result to the original session; a
  rename call also checks its requested name, including historical replay.
  Validated unapplied, uncertain, or missing receipts include recovery warnings
  without changing the returned receipt data or successful lookup status. Do not
  issue a new mutation ID to work around uncertainty.
- Root and `call` help include offline saved-data discovery and a bounded session
  listing example. Session creation help and dry-run previews explicitly say that
  successful creation also activates the new session. Offline schemas describe
  the CLI, not a guarantee of support in the selected running app; a validated
  unsupported-read response now suggests checking that pairing.
- Inspect `saved.v1` mutation receipts after uncertain outcomes. Legacy session
  and saved-tab metadata operations do not provide these receipts. Do not
  automatically repeat a mutation just because its response was lost or invalid.

See the [CLI integration guide](../integrations/codex.md),
[saved-data contract](../integrations/saved-data-v1.md), and
[saved-tab metadata operations](../integrations/saved-replay-tabs.md).

## Inspect and stage local skills

`skills status` compares installed files with the embedded bundle without
returning their text. A differing hash means `modified_or_outdated`; it does not
identify whether a person edited the file. `update-preview` compares an enrolled
baseline, so a manually updated file can be `current` in status and `modified`
in preview. Inspect the row states; exit zero does not mean every file is current.

`skills update-preview` is read-only. Explicit enrollment records a baseline;
experimental staging writes a separate candidate while preserving the active
file. Neither operation activates an update or enables unattended startup
replacement. Existing manual installation can overwrite an installed skill, so
preserve edits before choosing it. Enrollment receipts record a byte baseline,
selected path, agent, and bundle version; they do not grant account or network
access.

See [safe skill maintenance](../integrations/skill-maintenance.md) for exact
commands, refusal states, and activation and backup limitations.

## Preserve saved work across failures

Session metadata operations reconcile in-memory state with a visible registry
replacement even if a later finalization step fails. The original error still
matters: visibility is not proof of crash durability, and callers must not infer
that every failed operation was unapplied.

Ordinary and keepalive workspace saves also retain the visible committed revision
when finalization fails. They return HTTP 500 with
`code: "WORKSPACE_COMMIT_UNCERTAIN"`; callers must reload before retrying. When
the open GUI receives this response, it preserves local edits and pauses saves.
Copy newer edits before reloading the app. An unload-only keepalive response is
not observed by the closing page, so no recovery message is guaranteed there.
Pre-commit failures retain the prior workspace and existing error behavior.

Startup refuses ambiguous deletion quarantines before destructive cleanup or
registry repair. Preserve the entire sessions directory for recovery. This
startup safeguard does not automatically restore data or fence subsequent
writes by an already-running process. See
[interrupted session deletion](../architecture.md#interrupted-session-deletion).

Workspace synchronization now preserves confirmed metadata and concurrent
edits when an older save fails. Tab navigation and session create/open preserve
supported rendered editor drafts and ask the user to finish unsupported pending
edits before leaving. A session transition also stops if an earlier saved editor
becomes dirty during a later preflight save; edits actually acknowledged by later
saves remain eligible to proceed. External active-session changes preserve
unsupported editor drafts with their original workspace owner instead of
silently resetting them. Repeated preservation and passive synchronization do
not undo an unchanged HTTP-version selection; genuine edits still take priority.
Uncertain session-action failures refresh session rows without silently
rebinding the active editor. Conflict and recovery messages still require
attention; copy unrepresented local edits before manually reloading a session.
