# CLI and saved-workspace maintenance (unreleased)

This is an overview of maintenance work on `release/0.3.1`, not a release
announcement. The package version remains `0.3.0` until release preparation.

## Passive Service Map

The Scope UI and read-only CLI share a bounded inventory built from retained
capture metadata. Origins contain method-plus-exact-path endpoints, observed
counts and safe exact-row evidence. Header clues and redirect instructions are
labeled as observations; they do not establish infrastructure or a followed
request. No scans, probes, replay, or crawling are introduced. Check completeness
and unavailable data before drawing conclusions. See the
[Service Map contract](../integrations/service-map.md).

An optional, explicit one-row body-reference read is available through the
Service Map evidence inspector and `target.service_map.body_references.get`.
It requires the current active session and an exact-row token. Inactive sessions
remain unavailable without being loaded or activated. Only bounded retained
UTF-8 HTML/JavaScript previews are inspected; results expose canonical origins
and retained-preview offsets with separate capture/prefix truncation and cap
reasons. References do not establish calls or ownership. The default map and
metadata evidence still read no bodies, and nothing sends target requests.

The GUI can explicitly show one completed or partial body-reference result as a
temporary Direct relations overlay. Its pinned source and dotted arrows keep
literal references separate from traffic counts; the source method/path and
preview coverage remain visible. It adds no API or CLI operation, automatic read,
or persistence. All origins and the API catalog remain metadata-only. History or
source invalidation clears the overlay, including new transaction events; it is
not automatically restored. See the
[overlay lifecycle and display limits](../integrations/service-map.md#temporary-body-reference-overlay-gui-only).

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

## CLI PATH setup

Incomplete installation acknowledgements keep the error and recovery controls
visible instead of claiming successful registration. On macOS, Unicode padding
outside a managed command no longer gives that line automatic-edit ownership.
If appending would convert a final lone carriage return into a CRLF ending,
setup leaves that profile unchanged and asks for manual review.

macOS automatic PATH setup recognizes managed command lines without parsing shell
context. A managed-command example inside a here-document can be mistaken for
an existing registration or modified when the app path changes. This limitation
remains unresolved. If a profile contains such examples, use the bundled CLI by
its full path and review the profile manually before choosing automatic setup.

## UI reliability (2026-10-11)

- Display settings can be edited and previewed while their saved baseline loads,
  but Apply waits for a usable baseline. A failed or superseded initial read
  offers Retry without automatically applying the draft. If an explicit Apply
  cannot be confirmed, a warning explains that automatic save retries continue.
- Settings and Filter dialogs keep composition-marked Escape events within the
  text input. Saved-tab renaming likewise ignores composition-marked Enter and
  Escape. Stored labels are normalized to 80 Unicode scalar values without
  splitting supplementary characters; the existing inline typing cap is unchanged.
- A delayed note read no longer opens an editor after the user moves to another
  interaction. Opening and closing an unchanged saved note preserves its original
  line breaks; genuine inline edits still use the existing single-line editor.
  Reopening a row's context menu ignores older diagnostic-note reads.
  Status toasts expose polite announcements without moving focus.
  Clipboard fallback fields are removed even when selection or copying throws.
- Returning to the same finding or session does not revive an older detail
  read. Older list/count reads cannot replace a newer accepted badge count or
  undo an observed finding event; later valid lower counts still apply.
- Arrow keys already handled by the browser chooser no longer also move the
  session, history, finding, or frame selection underneath it. Findings row
  navigation also leaves arrow keys to focused text controls, modifier shortcuts,
  and open dialogs.
- App-update failure resets no longer overwrite a retry's progress or click guard.
- New or missing Notes widths default to 220px; explicit saved widths stay unchanged.

These changes have synthetic production-handler regression coverage. Native
desktop IME behavior, screen-reader announcements, and system clipboard
permission interactions still require manual verification.

Native close also waits for queued or in-flight note saves and refuses to finish
while a currently attached note editor has unqueued text or unfinished IME
composition. Finish or cancel the note edit before closing again. This does not
protect an unqueued draft whose editor has already been detached: virtual
scrolling can replace its row, and session or query resets have a separate editor
lifecycle.
In particular, a blurred composition can have neither a pending save nor a
visible editor after replacement. Finish note edits before these transitions;
recoverable detached-editor handling remains follow-up work. Native close/IME
event ordering has not been manually verified.

## Preserve saved work across failures

Successfully loaded UI settings files with an omitted or zero `server_revision`
receive revision 1 in memory. Loading does not rewrite the valid file. This
protects saved preferences from an uninitialized page's revision-0 snapshot;
clients should fetch the current settings and preserve the returned revision
when saving. A rejected stale write returns HTTP 200 with the existing snapshot,
so HTTP success alone does not prove that the requested changes were applied. Older
clients that never send the revision can now have their first write rejected
against such a file. Existing same-client version ordering remains unchanged.
Fresh-file creation and corrupt-file recovery retain revision 0 for that load;
their resulting valid files receive the same protection on a later restart.

Session metadata operations reconcile in-memory state with a visible registry
replacement even if a later finalization step fails. The original error still
matters: visibility is not proof of crash durability, and callers must not infer
that every failed operation was unapplied.

HTTP history snapshots publish body files and their read offsets together, while
preserving later record edits and annotations. Missing or mismatched stored
bodies stop persistence rather than becoming empty saved bodies. Ambiguous
repeated-ID histories with nonempty recovery journals now refuse loading and
preserve their files for review. See [transaction snapshot safety](../persistence-safety.md)
for cancellation, recovery, and supported session-lifecycle boundaries.

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
supported rendered workspace-editor drafts and ask the user to finish unsupported
workspace edits before leaving. A session transition also stops if an earlier saved editor
becomes dirty during a later preflight save; edits actually acknowledged by later
saves remain eligible to proceed. External active-session changes preserve
unsupported workspace-editor drafts with their original workspace owner instead of
silently resetting them. Repeated preservation and passive synchronization do
not undo an unchanged HTTP-version selection; genuine edits still take priority.
Uncertain session-action failures refresh session rows without silently
rebinding the active editor. Conflict and recovery messages still require
attention; copy unrepresented local edits before manually reloading a session.
