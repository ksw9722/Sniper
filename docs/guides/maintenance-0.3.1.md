# CLI and saved-workspace maintenance in 0.3.1

Maintenance work released in 0.3.1, in brief. The linked documents hold the
details.

## Service Map

The **Map** tool (after Capture, with **Service Map** and **APIs** views) and
`sniper-cli scope service-map` build a passive map of origins and endpoints from
captured history. They send nothing to a target. Check the map's completeness
and unavailable data before you draw conclusions. In the CLI, `target` is an
alias of `scope`. **Scope** again shows its rules beside the captured hosts and
paths.

0.3.1 added:

- a display filter that reuses HTTP history's filter dialog with its own saved
  settings, and takes the same conditions as options in the CLI. By default it
  hides images, styles and fonts.
- links counted by request kind (API call, page navigation, frame, script,
  static file, other), with frames, scripts and static files hidden by default,
  and CORS counts (`request_kinds`, `cors_allowed_count`,
  `cors_credentials_count`).
- origins grouped by site (on by default), and origins or sites hidden from the
  map. The API and the CLI still return every origin.
- per origin: front-end technologies, server technologies and exposures, clients,
  authentication and the captured favicon (`front_technologies`,
  `server_technologies`, `clients`, `auth`, `favicon`).
- endpoint parameters (`scope service-map parameters`), an OpenAPI 3.1 document
  for each API origin (**API docs** in the inspector, `scope service-map openapi`),
  and the signals no rule names (`scope service-map unnamed`).
- explicit one-row inspections: response-body references on the active session
  (`target.service_map.body_references.get`), Link declarations and recorded
  socket peers. The GUI collects up to eight of them in one graph for the session
  and never saves it.
- every retained row in the GUI map. The CLI reads a window unless you pass
  `--all-rows`.
- a layout by link depth, one arrow per direction between two cards, and zoom by
  trackpad pinch or Ctrl with the mouse wheel.

An origin whose rows have no typed response stays off the map unless you select
or search for it. See the [Service Map contract](../integrations/service-map.md).

## CLI contracts

- Saved-data reads check that each response matches the requested session,
  selection, page limit and continuation. Use explicit session IDs and pass
  returned continuations back unchanged.
- Parse and input-file failures return structured errors. Some failures now have
  different error or exit codes: branch on the documented contract, not on
  message text.
- Legacy session create, rename, switch and delete send the write once, with no
  redirects or retries. An unsuccessful or mismatched response reports an unknown
  outcome.
- Rule enable and disable and saved-tab label updates report an unknown outcome
  when the acknowledgement is malformed, incomplete or mismatched. They are not
  retried.
- After an unknown outcome, inspect the `saved.v1` mutation receipt. Do not repeat
  the write automatically or issue a new mutation ID. Legacy session and saved-tab
  metadata operations have no receipts.
- Creating a session also activates it; help and dry-run previews now say so.
  Offline schemas describe the CLI, not what the running app supports.
- `replay open` and `replay update` give a request from `--request-file` or
  `--stdin` the target scheme, so plain-HTTP replay to `127.0.0.1` works from the
  CLI. See
  [Replaying a request with a change](http-debugging.md#replaying-a-request-with-a-change).

See the [CLI integration guide](../integrations/codex.md), the
[saved-data contract](../integrations/saved-data-v1.md) and
[saved-tab metadata operations](../integrations/saved-replay-tabs.md).

## Skill status

`skills status` compares installed skill files with the bundle in the binary,
without returning their text. `modified_or_outdated` means the hash differs; it
does not say whether a person edited the file. Read each row's state: exit
status 0 does not mean every file is current.

## CLI PATH setup

- If installation is not fully acknowledged, the error and recovery controls stay
  visible instead of reporting success.
- On macOS, if appending would turn a final lone carriage return into a CRLF
  ending, setup leaves the profile unchanged and asks for a manual review.
- macOS automatic setup finds managed command lines without parsing shell syntax,
  so a managed-command example inside a here-document can be taken for a
  registration, or changed when the app path changes. If your profile has such
  examples, run the bundled CLI by its full path and review the profile yourself
  before you choose automatic setup.

## UI reliability

- Display settings can be edited and previewed while the saved settings load;
  **Apply** waits until they have loaded. A failed first read offers **Retry**
  and does not apply the draft. If an Apply cannot be confirmed, a warning says
  that automatic save retries continue.
- During IME composition, Escape no longer closes the Settings or Filter dialog,
  and Enter or Escape no longer ends saved-tab renaming. Stored labels are
  limited to 80 Unicode scalar values without splitting a character.
- A late read no longer opens a note editor after you have moved on, brings back
  an older finding or session detail, or replaces a newer badge count.
- An unchanged saved note keeps its line breaks when you open and close it.
- Arrow keys that the browser chooser handles no longer also move the selection
  under it. Findings row navigation leaves arrow keys to text fields, shortcuts
  and open dialogs.
- Status toasts make polite screen-reader announcements without moving focus.
- The Notes column defaults to 220px; saved widths stay.
- Closing the app waits for pending note saves and refuses to finish while an
  open note editor has unsaved text or IME composition. Finish or cancel the edit
  first. A draft whose editor was already replaced, by scrolling or by a session
  or query reset, is not protected, so finish note edits before those.

## Saved work across failures

- UI settings files with a missing or zero `server_revision` load as revision 1;
  the file is not rewritten. Clients should fetch the current settings and send
  back the returned revision when saving. A rejected stale write returns HTTP 200
  with the stored settings, so HTTP 200 alone does not mean the change was
  applied. An older client that never sends the revision can have its first
  write rejected.
- A failed session metadata operation may still have taken effect.
- HTTP history snapshots save body files and their offsets together. Missing or
  mismatched bodies stop persistence instead of being saved empty. A history with
  repeated IDs and nonempty recovery journals refuses to load and keeps its files
  for review. See [transaction snapshot safety](../persistence-safety.md).
- A workspace save that fails after commit returns HTTP 500 with
  `code: "WORKSPACE_COMMIT_UNCERTAIN"` and keeps the committed revision. Reload
  before retrying. The GUI keeps local edits and pauses saves; copy newer edits
  before you reload the app.
- Startup refuses ambiguous deletion quarantines before any cleanup. Keep the
  whole sessions directory for recovery. See
  [interrupted session deletion](../architecture.md#interrupted-session-deletion).
- Workspace drafts survive failed saves, tab changes and session changes. When
  Sniper asks you to finish an edit before leaving, do so. After a conflict or
  recovery message, copy your local edits before you reload a session.

## Observed socket peer capture

A switch in Capture Settings, and `capture http upstream-peer get/set`, lets the
active session record up to one socket peer observation per new eligible
HTTP(S) response. It is off by default. Service Map evidence shows the
observation only with `--include-socket-peer`. Observations are saved in session
files, exports and full-record JSON, and turning the switch off does not erase
them. See the
[observed socket peer contract](../integrations/observed-socket-peer.md).
