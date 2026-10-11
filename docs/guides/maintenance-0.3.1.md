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
- Parse and local input-file failures have structured errors. Non-Unicode
  argument bytes are suppressed, and input JSON loading/parsing failures do not
  echo their contents; ordinary argument diagnostics and file paths may still
  appear. After API discovery, failed generic API reads use the actual HTTP
  status without echoing server error bodies. Some failures consequently have
  different error codes or exit codes; branch on the documented contract, not
  message fragments.
- Legacy session create, rename, switch, and delete send the mutation once without
  following redirects or retrying it. Rename and switch also require the returned
  session UUID to match the requested UUID; equivalent spellings remain accepted.
  A mismatched acknowledgement leaves the mutation outcome unknown.
- Inspect `saved.v1` mutation receipts after uncertain outcomes. Legacy session
  and saved-tab metadata operations do not provide these receipts. Do not
  automatically repeat a mutation just because its response was lost or invalid.

See the [CLI integration guide](../integrations/codex.md),
[saved-data contract](../integrations/saved-data-v1.md), and
[saved-tab metadata operations](../integrations/saved-replay-tabs.md).

## Inspect and stage local skills

`skills status` compares installed files with the embedded bundle without
returning their text. A differing hash means `modified_or_outdated`; it does not
identify whether a person edited the file.

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

Startup refuses ambiguous deletion quarantines before destructive cleanup or
registry repair. Preserve the entire sessions directory for recovery. This
startup safeguard does not automatically restore data or fence subsequent
writes by an already-running process. See
[interrupted session deletion](../architecture.md#interrupted-session-deletion).

Workspace synchronization now preserves confirmed metadata and concurrent
edits when an older save fails. Tab navigation and session create/open preserve
supported rendered editor drafts and ask the user to finish unsupported pending
edits before leaving.
Uncertain session-action failures refresh session rows without silently
rebinding the active editor. Conflict and recovery messages still require
attention; copy unrepresented local edits before manually reloading a session.
