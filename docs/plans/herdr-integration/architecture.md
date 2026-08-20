# Architecture

Design for the locked scope in [scope.md](scope.md). Nothing described here is
implemented. Repository line references reflect the tree as read during
planning and are starting points, not guarantees.

## Shape of the change

Two new homes plus narrow, additive seam edits in existing files. Herdr domain
and protocol state live in one new pure crate. Presentation lives in one new
desktop glue module plus one new sidebar renderer. Everything else is a single
additive branch in an existing upstream file, which keeps the fork cheap to
merge with upstream.

## Module ownership

| Module | Status | Owns |
|---|---|---|
| `crates/herdr_core/` (package `termy_herdr_core`) | new, GM-gated (NF1) | Branded domain types, trust policy, service supervision, transport seam and its fake, background session thread, catalog mirror, `HerdrController` facade |
| `crates/desktop_app/src/terminal_view/herdr/` | new, GM-gated (NF2) | Sidebar-view state, per-frame event drain, Agent Tab creation and byte feed, detach-on-close hook, Native Runtime gate, window-local dedup |
| `crates/desktop_app/src/terminal_view/tab_strip/render_herdr_sidebar.rs` | new, GM-gated (NF3) | Herdr view rendering, beside the existing `render_workspace_sidebar.rs` |
| `crates/config_core/` | existing, edited | Four typed root settings, parse only |
| `crates/desktop_app/src/terminal_view/` | existing, edited | Tab identity and dedup, the `Option<HerdrRuntime>` field, the drain call |
| Herdr Service | external, unchanged | All Herdr state. Termy holds a derived mirror only |

## The pure core boundary

`herdr_core` is a pure domain and protocol crate with zero GPUI and zero
desktop dependencies, following the existing `termy_ssh_core` precedent
(`crates/ssh_core/src/lib.rs`, wired as a path dependency in
`crates/desktop_app/Cargo.toml:58`). It never sees `TabId` or any GPUI type.

Its modules:

- `domain.rs`: branded ids `SpaceId`, `AgentId`, `AgentKey { space, agent }`,
  the phase enum with exactly `Starting | Running | WaitingForInput | Succeeded
  | Failed`, `AgentCommand { program, argv }` with no shell-string type in
  existence, `RequestId`, `ConflictId`, and a `Confirmed` witness type.
- `trust.rs`: `TrustedLocation::vet(candidate, trusted_set) ->
  Result<TrustedLocation, TrustRejection>` is the only constructor of a
  launchable location. It canonicalizes, requires absolute, existing,
  executable, and inside the trusted set. `TrustRejection` carries the rejected
  path so the UI can show it (S2). No other API accepts a path. Executability
  probing follows the no-PATH pattern already used for CLI delegation
  (`crates/desktop_app/src/cli_delegate.rs:81-110`); the vetting matrix is
  written fresh rather than inherited, because the existing helper treats any
  regular file as executable. Documented install locations and the bundled
  Service location are data constants owned here.
- `transport.rs`: a private `HerdrTransport` trait. Typed commands out, typed
  notifications in. Wire types never escape the crate.
- `fake.rs`: an in-memory `HerdrTransport` implementation, the standing test
  double the repository does not otherwise have. It records input sent per
  attachment so tests can assert on it.
- `session.rs`: the background thread owning all blocking work, plus the derived
  catalog mirror and reconnect policy.
- `service.rs`: probe, then trusted launch, then origin tracking.
- `lib.rs`: the `HerdrController` facade.

Controller surface:

```
spawn(config) -> HerdrController          // never blocks; failures arrive as events
catalog()                                 // read the mirror
create_agent(SpaceId, AgentCommand)       // S6, no shell string exists
request_attach(AgentKey) -> AttachTicket
resolve_control(ConflictId, Choice)       // TakeOver(Confirmed) | Observe | Cancel, S10
detach(AgentKey)                          // S8, never stops
close_agent(AgentKey, Confirmed)          // S9, the only stopping call
poll() -> Vec<HerdrEvent>                 // drained once per frame
```

Every mutation carries a request id and is idempotent under retry, so a lost
reply that gets re-sent cannot double-create an Agent or double-seize control.

## Typed writable attachment

Input capability is a type, not a policy check.

`request_attach` resolves to `Writable | Conflict(holder) | Unavailable`. Only a
`Writable` resolution yields a `WritableAttachment`, and `WritableAttachment` is
the only type in the crate that exposes the transport's attachment input
operation. No binding means no input path exists to call.

This makes the Observe state structurally read-only rather than policed. An
Observe attachment simply lacks the input operation, so the desktop input seams
have nothing to write to, and no later code path can restore input without
changing the type. A `Conflict` yields no tab and no input sink at all until the
user chooses Take Over, Observe, or Cancel (S10).

## Catalog snapshots and events

The Herdr Service owns all Herdr state. Termy holds a derived, versioned
whole-catalog mirror: a whole snapshot plus a revision, with no delta index.
This mirrors the existing tmux precedent, where a `NeedsRefresh` signal triggers
a whole refresh (`crates/desktop_app/src/terminal_view/runtime/tmux/events.rs:193-196`).

Rendering the Herdr view is a pure function of the mirror. The phase vocabulary
is exactly the five values in S5. Control ownership and connection loss are
separate fields rendered as separate markers, never folded into a phase.

Data flows one way. The UI sends typed commands to the controller. Notifications
(snapshot, output, control change, connection change) flow back through the
drain. Service stdout and stderr are diagnostics only. Only attachment output
bytes reach a terminal display, so status markers never enter the byte stream.

## Per-frame drain and desktop glue

`TerminalView` stays synchronous. It holds `herdr: Option<HerdrRuntime>` and
drains controller events once per frame, at the same call site that already
drains tmux terminal events
(`crates/desktop_app/src/terminal_view/mod.rs:4482-4483`, which calls the tmux
event processing in `runtime/tmux/events.rs:175-247`). Disabled is absence: the
field is `None`, and no detection, process, or connection code path runs at all
(S1). Dropping the runtime never stops the Service or its Agents.

The glue module owns the sidebar-view state, `open_or_focus_agent(AgentKey)`,
the lifecycle hooks, and the map from `AgentKey` to pane identity. Keeping that
map inside the glue avoids adding a field to the shared tab or pane structs.

`open_or_focus_agent` enforces, in this order:

1. Native Runtime gate. On tmux the user gets a requirement message through the
   existing toast surface and nothing mutates, following the precedent already
   used when adding an SSH tab
   (`terminal_view/tabs/lifecycle.rs:481-486`, `ui/toast.rs:249`).
2. Window-local dedup. A scan over the active `session.tabs` plus every stashed
   `WorkspaceEntry.tabs` (`terminal_view/workspaces.rs:7-24`, stash and restore
   at `462-484`), focusing an existing tab even when parked in an inactive
   Workspace, using the same position-by-id approach the command palette
   already uses (`terminal_view/command_palette/plugins.rs:1397-1401`). There is
   no second Agent-to-tab map that could drift.
3. Attach, then construct the tab.

## Agent Tab terminal

The desktop `Terminal` enum today has `Tmux(PaneTerminal)` and
`Native(NativeTerminalInstance)` and lives in
`crates/desktop_app/src/terminal_view/backend.rs:7-10`. `PaneTerminal`
(`crates/terminal_ui/src/pane_terminal.rs:64`) is already a PTY-less,
externally-fed display terminal, constructed without a child process for the
tmux path.

The Agent Tab adds a third fork-only variant, `HerdrAgent(PaneTerminal)`, fed by
the drain. Every exhaustive `Terminal` policy gains an explicit named arm, with
no catch-all, so the compiler is the sweep detector. Display-side methods
delegate to the wrapped `PaneTerminal` exactly as the tmux arms do. PTY-only
methods take an explicit none or no-op arm. `write_input` and
`write_input_owned` are explicit no-ops for `HerdrAgent`, because Agent input
never travels that route: it goes through the writable attachment binding.

The native PTY path (`terminal_view/mod.rs:507`) and the `Program` launch type
(`crates/core/src/runtime.rs:162-165`) are not touched, so the SSH flow stays
the only consumer of a program launch.

## Exact existing seams

| File | Edit |
|---|---|
| `crates/config_core/src/types.rs`, `schema.rs`, parser and parser tests | Four typed root settings, parse only, no filesystem work at parse time |
| `crates/desktop_app/src/terminal_view/render.rs:3778-3780` | One sidebar-view branch where `workspace_sidebar_visible()` already gates `render_workspace_sidebar` |
| `crates/desktop_app/src/terminal_view/mod.rs` | `Option<HerdrRuntime>` field; drain call at `4482-4483`; the `impl Terminal` policy arms at `471-1000` |
| `crates/desktop_app/src/terminal_view/backend.rs` | The `HerdrAgent` enum variant at `7-10` and a named arm in `terminal_engine_label` at `158-165` |
| `crates/desktop_app/src/terminal_view/tabs/lifecycle.rs` | Agent Tab open and close hooks |
| `crates/desktop_app/src/terminal_view/interaction/input.rs:259-283` | Agent-tab branch in `send_input_to_pane` and `send_owned_input_to_pane` writing to the binding |
| `crates/desktop_app/src/terminal_view/interaction/mouse_reporting.rs:211-243` | Same branch for `send_mouse_packet_to_pane` and `send_owned_mouse_packet_to_pane` |
| `crates/desktop_app/src/terminal_view/persistence.rs` | Exclude Agent panes from the native workspace snapshot |
| `crates/desktop_app/src/settings_view/sections.rs` | Rows for enablement, service path override, trusted paths |
| `crates/desktop_app/Cargo.toml` | Path dependency on the new core crate, beside `termy_ssh_core` at line 58 |

Patching the two input seams rather than their callers is deliberate. Higher
level input paths funnel through them: paste reaches `send_owned_input_to_pane`,
and plugin-directed input resolves an arbitrary pane by id and then calls
`send_input_to_pane`. Guarding the seams covers those callers for free.

## Persistence

The enablement preference and the last selected sidebar view are root settings
written through the existing atomic config writer
(`crates/desktop_app/src/config/mutate.rs:135`, upsert at
`crates/config_core/src/document.rs:14-58`), the same path the app already uses
for theme settings.

The workspace SQLite store is not touched and must not become a Herdr
dependency. It is gated by an unrelated pair of settings
(`terminal_view/persistence.rs:288,352`) and needs no schema change, because
Agent Tab restart survival is out of scope.

Agent panes are excluded from the native workspace snapshot. Quit serializes the
native workspace (`interaction/quit.rs:295-305`), serialization currently walks
every tab and pane with no purpose filter (`persistence.rs:539-584`), and
restore rebuilds every saved pane as a Native terminal
(`persistence.rs:785-815`, `878-892`). Without the exclusion, an Agent Tab would
come back after relaunch as an ordinary dead terminal. One exclusion point
covers both the active strip and stashed Workspaces, because both route through
the same collection function.

## State and invariants

- **Disabled is absence.** `None` runtime, no code path (S1).
- **Service lifecycle.** `Probing -> Connecting(origin) -> Ready(origin) |
  Failed(reason)`, with `Starting(TrustedLocation)` reachable only from a failed
  probe, which is what makes connect-first structural (S2). `origin` is
  `Existing` or `StartedByTermy` and feeds the disclosure line, nothing else.
- **Catalog.** Whole snapshot plus revision. Render is a pure function of the
  mirror (S5).
- **Attachment.** `Requested -> Writable | Conflict(holder) | Unavailable`.
  Conflict yields no tab and no sink before an explicit choice (S10).
- **Tab purpose is typed.** An Agent Tab carries `AgentKey` plus attachment
  state. An ordinary native tab cannot become one. Ordinary close on an Agent
  Tab detaches. Only `close_agent` with a `Confirmed` witness stops and removes
  (S8, S9). Quit reuses the same detach path and the quit flow gains no Herdr
  stop call.
- **Sidebar.** Exactly two mutually exclusive views over one sidebar. The
  Workspace branch calls the unchanged renderer (S3).
- **Herdr first exactly once.** Sidebar choice and first-presentation history
  are separate domain facts. The visible selector may store a view while the
  integration is disabled, so view-setting absence cannot safely carry the S4
  invariant. A persisted `herdr_initial_view_presented` marker starts false and
  changes only when an initial auto-enable forces the Herdr view. That launch
  stores the Herdr view and sets the marker. Later launches restore the saved
  view without forcing it again. Auto-enable never writes the enablement
  preference, because that preference is reserved for the user's explicit
  choice.
- **Idempotency.** Create, control resolution, detach, and close all converge
  under retry through request ids.

## Failure behavior

| Failure | Owner | User-visible result | State |
|---|---|---|---|
| No saved preference, no trusted executable | enablement resolver | Nothing. Launch is upstream-identical | Disabled, runtime absent |
| Untrusted path | trust gate | Explanatory state showing the rejected path | Nothing launched |
| Probe finds no Service and trusted start fails | session thread | Service-unavailable state | Failed, retry on user action |
| Handshake or protocol mismatch | session thread | Incompatible-local-service state | No wire value enters the domain |
| Transport drop mid-session | session thread | Catalog stays visible with a disconnected marker, tabs keep scrollback | Reconnect with backoff, reattach replays idempotently |
| Create fails | controller | Form stays open with the error, no Agent row | No optimistic mutation |
| Agent gone before attach | controller | Catalog refresh, no tab | Converged |
| Open in tmux runtime | `open_or_focus_agent` | Native Runtime requirement message, nothing changes | No mutation |
| Close Agent fails or reply lost | controller | Agent and tab remain with the error, repeat converges to absent | Idempotent |
| Empty catalog | Herdr view | Explanatory empty state | No error |

## Explicitly excluded

- The workspace SQLite store and its schema.
- The native PTY launch path and the `Program` launch type. SSH stays their only
  consumer.
- The internals of the existing Workspace sidebar renderer, the tmux runtime
  modules, quit-flow semantics, and upstream Workspace behavior.

## Risks

| Risk | Handling |
|---|---|
| Protocol contract facts are unconfirmed: replay-then-live as one ordered stream per attachment, the mutation idempotency key, and the controller identity string for Take Over text | The transport seam and its fake pin the assumed contract. If the real protocol diverges, only `transport.rs` and `session.rs` change. Confirm against Herdr protocol documentation before building the real transport |
| `PaneTerminal` carries tmux-specific interception (kitty graphics, prompt repaint) that could misrender Agent output | Retired early by a dedicated rendering proof before any UI builds on it. Fallback is a construction flag on `PaneTerminal`, which is a seam edit and not a new file |
| Documented install locations and the bundled Service location are not yet pinned | They are data constants owned by the trust module. No structure depends on their values |
| Merge drift with upstream | Seam edits are single additive branches in a small named set of existing files. A periodic upstream merge re-verifies only those seams |
| GM ticket gate | The three new-file groups block their phases. Planning proceeded; implementation cannot start |
