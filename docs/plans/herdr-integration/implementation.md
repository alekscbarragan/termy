# Implementation roadmap

Approved plan for the locked scope. Thirteen phases, P1 to P13, and
thirty-four tasks, T1 to T34. The identifiers are stable and are never added,
removed, split, or renumbered without a fresh human-approved design return.

**Nothing below has been implemented.** This document is the roadmap that a
future implementation follows, not a record of work done.

## Execution gates

1. **GM ticket for NF1-NF3.** No ticket exists. It blocks P2, P3, P4, P6, and
   every later phase that depends on them. Only P1 and P5 touch existing files
   only and may start before the ticket exists. No ticket ID is invented here,
   and none of the gated logic may be moved into existing files to evade the
   gate.
2. **Protocol contract confirmation before P9.** Confirm against Herdr protocol
   documentation: replay-then-live as one ordered stream per attachment, the
   mutation idempotency key, and the controller identity string used in Take
   Over text. Divergence changes only `transport.rs` and `session.rs`.
3. **Single implementation owner.** One owner holds the diff across all phases.
   Parallel agents, if any, take whole phases in isolated worktrees and never
   share files. Reviewers grade and return findings to the owner.

### New production files behind gate 1

| ID | Path | Created in |
|---|---|---|
| NF1 | `crates/herdr_core/Cargo.toml` plus `src/{lib,domain,trust,service,transport,session,fake}.rs` (one ticket covers the crate) | P2, extended in P3, P4, P9 |
| NF2 | `crates/desktop_app/src/terminal_view/herdr/mod.rs` | P6 |
| NF3 | `crates/desktop_app/src/terminal_view/tab_strip/render_herdr_sidebar.rs` | P6 |

### Historical execution constraints

The planning run that produced this packet made no commits and took no remote
action, and the plan text authorizes none for its own execution. That constraint
described the planning run. It says nothing about publishing this documentation,
which is separately authorized. A future implementation will need its own
commit authorization from the maintainer before any per-phase committed
checkpoint can be satisfied.

## Recorded assumptions

| ID | Assumption | Retired by |
|---|---|---|
| A-1 | Attach replay-then-live arrives as one ordered stream, and mutations accept an idempotency key | Gate 2, before P9 |
| A-2 | Documented install locations and the bundled Service location are data constants; nothing structural depends on their values | T5 |
| A-3 | Agent byte streams render cleanly through `PaneTerminal` | P5, before any UI builds on it |

## Phase roadmap

| Phase | Slice | Delivers | Gated by |
|---|---|---|---|
| P1 | 1a | Config parse for four settings | none |
| P2 | 1b | Core crate scaffold, domain types, trust gate | GM ticket (NF1) |
| P3 | 1b | Transport seam, fake, catalog mirror | GM ticket (NF1) |
| P4 | 1b | Controller mutations and attachment lifecycle | GM ticket (NF1) |
| P5 | 1c | `Terminal::HerdrAgent` reuse proof | none |
| P6 | 1d | Runtime field, drain, sidebar selector, Herdr view | GM ticket (NF2, NF3) |
| P7 | 1d | Create form, open-or-focus, dedup, tmux gate | P6 |
| P8 | 1d | Detach on close, quit detach, confirmed Close Agent | P7 |
| P9 | 1e | Real transport, supervision, gated integration test | P8, gate 2 |
| P10 | 2 | Enablement detection, Herdr-first-once, view restore | P9 |
| P11 | 2 | Settings UI rows | P10 |
| P12 | 3 | Control-conflict flow | P9 |
| P13 | 4 | Failure, empty, and reconnect states; preservation pass | P12 |

Slice 1 is P1 to P9 and is the first usable milestone: the complete local Agent
loop for a user whose preference is already saved. P1 to P8 end in safe stops
because the integration is inert while unconfigured. Only P9's boundary is a
shippable state.

Complexity ascends within each slice. Every phase ships a type together with its
first consumer and its test.

## Phases and tasks

### P1. Config settings parse (S1, S2, S4)

Files: `crates/config_core/src/types.rs`, `schema.rs`, the parser, and parser
tests. Parse only, with no filesystem work at parse time.

| Task | Clauses | Work |
|---|---|---|
| T1 | S1, S2, S4 | Add five typed root settings: enablement as an optional bool where absent means no saved preference, an optional absolute service-path override, a trusted-paths string list, an optional two-value sidebar view, and `herdr_initial_view_presented` as a persisted boolean whose absent or false state means the initial auto-enable presentation has not happened. The presentation marker is internal UX state, not a second enablement control. A present but invalid sidebar value parses as a stored `workspaces` choice. |
| T2 | S1, S4 | Parser tests beside the existing boolean pattern: absent enablement parses to no-preference; each setting round-trips; sidebar view absence, explicit `workspaces`, and explicit `herdr` remain distinct; an invalid value parses as a stored `workspaces` choice; the presentation marker defaults false and round-trips true. |

Verify: `cargo test -p termy_config_core`.
Safe stop: settings parse and nothing consumes them. App behavior unchanged.

### P2. Core crate scaffold, domain, trust (S1, S2) [NF1]

Files: the new crate plus the path dependency in `crates/desktop_app/Cargo.toml`.
Zero GPUI and zero desktop dependencies.

| Task | Clauses | Work |
|---|---|---|
| T3 | S5, S6, S9, S10 | Crate scaffold plus branded domain types: `SpaceId`, `AgentId`, `AgentKey`, the five-value phase enum, `AgentCommand { program, argv }` with no shell-string type in existence, `RequestId`, `ConflictId`, and the `Confirmed` witness. |
| T4 | S2 | `TrustedLocation::vet` as the sole constructor of a launchable location: canonicalize, require absolute, existing, executable, inside the trusted set. `TrustRejection` carries the rejected path. Rejection matrix covering relative, PATH-resolved only, missing, non-executable, symlink escaping the set, and absolute but outside the set. The matrix is written for this gate rather than inheriting the permissive existing executable check. |
| T5 | S1, S2 | Pin documented install locations and the bundled Service location as data constants, cited to the Herdr install documentation in the test that asserts the list. Retires A-2. |

Verify: `cargo build -p termy_herdr_core`, `cargo test -p termy_herdr_core trust`.
Safe stop: pure crate compiles with the trust gate proven, no caller outside tests.

### P3. Transport seam, fake, catalog mirror (S5) [NF1]

| Task | Clauses | Work |
|---|---|---|
| T6 | S5, S6 | Private `HerdrTransport` trait: typed commands out, typed notifications in (snapshot, ordered replay-then-live output, control change, connection change). The command set includes a typed attachment input operation carrying verbatim bytes for one writable attachment, alongside create, attach, detach, and close. Without it no Agent Tab could ever send input and Observe enforcement would have no seam to guard. Wire types never escape the crate. The fake implements the trait in memory and records input per attachment. |
| T7 | S5 | Session state plus the derived catalog mirror: whole snapshot with revision, no delta index. Reads are a pure function of the mirror. Tests against the fake: snapshot replaces the mirror, revision is monotonic, the phase vocabulary is exactly the five values, ownership and connection loss are separate fields. |

Verify: `cargo test -p termy_herdr_core session`.
Safe stop: seam and fake proven, no real I/O exists.

### P4. Controller mutations and attachment lifecycle (S6, S8, S9, S10) [NF1]

| Task | Clauses | Work |
|---|---|---|
| T8 | S6 | `create_agent` with a request id, idempotent under retry so a re-sent lost reply cannot double-create. A failed create returns a typed error and leaves the catalog unchanged. |
| T9 | S10 | `request_attach` with lifecycle `Requested` to `Writable`, `Conflict(holder)`, or `Unavailable`. A writable resolution yields a `WritableAttachment`, the only type exposing the T6 input operation, so input capability exists exactly where a binding exists. Slice 1 policy: a conflict surfaces as unavailable, meaning attach is refused and never a silent takeover, until P12 lands resolution. |
| T10 | S8, S9 | Distinct verbs: `detach` never stops; `close_agent` with a `Confirmed` witness is the only stopping call and converges to absent under retry. `poll` drains events for the frame loop. |

Verify: `cargo test -p termy_herdr_core`.
Safe stop: full controller policy proven against the fake, nothing in the app calls it.

### P5. `Terminal::HerdrAgent` reuse proof (S7, S8; retires A-3)

Existing files only, so this phase is runnable before the GM ticket.

| Task | Clauses | Work |
|---|---|---|
| T11 | S7 | Add the third fork-only variant to the `Terminal` enum in `backend.rs`, which is where the enum actually lives. Every exhaustive `Terminal` policy gains an explicit named arm and no catch-all: a constructor beside the tmux one; output feed and hydrate delegating to the wrapped `PaneTerminal` exactly as the tmux arms do; every display-side method taking the tmux-equivalent delegation, including the grid accessor that terminal selection depends on; PTY-only methods taking an explicit none or no-op arm; `write_input` and `write_input_owned` as explicit no-ops because Agent input routes through the attachment binding instead; and a named arm in `terminal_engine_label`. The compiler is the sweep detector, so the phase completes only when the workspace compiles with a named arm in every formerly exhaustive match. The native PTY path and the `Program` launch type stay untouched. |
| T12 | S7 | Rendering test: feed agent-style output (ANSI colors, carriage-return progress, prompt repaint, kitty-graphics sequences) through the new variant and assert clean rendering, guarding the tmux-specific interception the existing pane tests cover. Compile proof accompanies it: no `Terminal` match acquired a catch-all to absorb the variant. If rendering is red, apply the A-3 fallback inside this phase. |

Verify: `cargo test -p termy herdr_agent`.
Safe stop: the variant exists with proof, no tab is ever constructed with it.

### P6. Runtime field, drain, sidebar selector, Herdr view (S1, S3, S5) [NF2, NF3]

| Task | Clauses | Work |
|---|---|---|
| T13 | S1, S5 | `TerminalView.herdr: Option<HerdrRuntime>`, where `None` means disabled and no Herdr code path runs. This phase constructs it only from a saved enabled preference; detection lands in P10. Per-frame drain beside the tmux drain, feeding controller events into the mirror and into any attached Agent terminals. |
| T14 | S3, S5 | The two-state sidebar selector rendered at the existing branch, where the Workspaces arm calls the unchanged Workspace renderer. NF3 renders Spaces and Agents from the mirror: phase from the five-value vocabulary, ownership and connection loss as separate markers, plus the origin disclosure line when Termy started the Service. |
| T15 | S3, S5 | Desktop state tests beside the existing sidebar suite: two-view exclusivity, Workspace-render preservation, and a live status change reflected in the mirror-driven render input. |

Verify: `cargo test -p termy herdr`; `cargo test -p termy workspaces` stays green.
Safe stop: users enabled by saved preference see a live Herdr view, no mutation paths exist.

### P7. Create form, open-or-focus, dedup, tmux gate (S6, S7)

| Task | Clauses | Work |
|---|---|---|
| T16 | S6 | Create-Agent form on a Space row: exact program and argument-list inputs, no shell-string interpretation anywhere. A failed create leaves the form open with the error and adds no Agent row, with no optimistic mutation. |
| T17 | S7 | `open_or_focus_agent` enforcing, in order: the Native Runtime gate with a requirement toast and zero mutation on tmux; window-local dedup by scanning the active tabs plus every stashed Workspace, focusing an existing tab even when parked in an inactive Workspace; then attach and construct the Agent tab carrying its `AgentKey` identity plus the writable binding from its attach. Agent-tab keyboard, paste, and mouse input route through that binding at the two existing desktop input seams, which gain an Agent-tab branch writing to the binding instead of the terminal. A tab carrying no binding accepts no input at those seams. Proof against the fake: keystrokes and mouse packets on an Agent tab reach the fake through the binding and none reaches terminal input. |
| T18 | S6, S7 | Tests: dedup focuses across active and stashed Workspaces; tmux rejection mutates nothing; a failed create leaves the form open with no row; exact-argv preservation, creating an Agent whose argument list contains tokens with spaces and shell metacharacters (for example `a b`, `; echo x`, `$HOME`, `"quoted"`) and asserting the fake received the identical token list with no splitting, joining, or expansion. |

Verify: `cargo test -p termy herdr`.
Safe stop: create and open work end to end against the fake-backed controller.

### P8. Detach on close, quit detach, confirmed Close Agent (S8, S9)

| Task | Clauses | Work |
|---|---|---|
| T19 | S8, S9 | Ordinary close of an Agent Tab maps to detach only. A distinct Close Agent action with its own confirmation carries the `Confirmed` witness, stops the Agent, and closes the tab. |
| T20 | S8, S9 | Quit reuses the same detach path for every open Agent Tab, and nothing in Termy stops the Service. Agent panes are additionally excluded from native workspace persistence, because quit persists the native workspace, serialization walks every tab and pane with no purpose filter, and restore rebuilds every saved pane as a Native terminal. Agent panes must never be serialized and never restored as Native terminals; after a relaunch the Agent reopens only through the Herdr view. Tests: ordinary close never stops; quit detaches only; only the confirmed action stops and removes; a persist-then-restore round trip of a Workspace containing an Agent tab **succeeds**, with non-Agent tabs and their layout trees intact and no Native terminal standing in for the Agent. |

Verify: `cargo test -p termy herdr`.
Safe stop: slice 1d complete against the fake; the loop lacks only the real transport.

### P9. Real transport, supervision, gated integration test (S2, S6, S8, S9) [milestone]

Prerequisite: gate 2 confirmed.

| Task | Clauses | Work |
|---|---|---|
| T21 | S2 | Real transport behind the unchanged seam, plus service supervision on the background session thread: probe for a running Service first, let only a failed probe reach the starting state, launch only what the trust gate returns, and record whether the Service was already running or started by Termy for the disclosure line. First-time start is silent. |
| T22 | S2, S6, S8, S9 | One gated integration test that skips without a Herdr binary, isolated the way the existing tmux harness isolates itself. Probe-before-start and trusted launch, then two Agents with disjoint acceptance. Agent A: create, replay, live output, confirmed close, assert absent. Agent B: create with an argument list containing spaces and shell metacharacters and assert the argv token list the Agent actually received through the real transport is identical; live output, detach, reattach showing detached-period output; then disconnect the test client entirely, reconnect fresh, rediscover Agent B, and reattach with output accumulated while disconnected. Agent B is never confirmed-closed, and the Service and Agent B are alive at test end. The same Agent is never used for both confirmed close and survival. |
| T23 | S2, S6, S7, S8, S9 | Manual end-to-end loop in the real app: enable by saved preference, list, create with an argument list containing spaces and shell metacharacters and verify the running Agent received them verbatim, open, detach, reattach. Then quit Termy entirely, relaunch a fresh process, rediscover the same Agent in the Herdr view, reattach showing output accumulated while detached, and continue live, confirming no restored Native terminal stands in for the Agent tab. Exercise confirmed Close Agent on a different Agent than the quit-and-relaunch one. |

Verify: `cargo test -p termy_herdr_core --test <integration>` with a local Herdr
binary (skip without one), plus the manual loop.
Safe stop and first usable milestone: the complete local Agent loop works for a
saved-preference user, main green.

### P10. Enablement detection, Herdr-first-once, view restore (S1, S4)

| Task | Clauses | Work |
|---|---|---|
| T24 | S1 | Single resolver at `TerminalView` construction: a saved preference wins; with none, probe only the explicit absolute override and the T5 documented locations, never inherited PATH; a trusted executable enables this launch; none found means disabled and zero further Herdr work. Auto-enable never writes the enablement preference, which is reserved for the user's explicit choice, so detection legitimately repeats each launch until the user saves one. |
| T25 | S4 | The Herdr-first force keys on `herdr_initial_view_presented`, not on sidebar-setting absence or the auto-enable preference. When fresh-config detection auto-enables Herdr and the marker is false, show the Herdr view, store that view, and set the marker true in the same settings update. Selector choices made while the integration is disabled may update the stored view but never the presentation marker. Once the marker is true, every later launch restores the stored view without forcing Herdr again. |
| T26 | S1, S4 | Probe-spy test proving a disabled startup performs zero probes, launches, and connections. Sequence tests: a disabled user selects Workspaces before any auto-enable and the marker remains false; a later fresh-config auto-enable still forces Herdr and sets the marker exactly once; a second launch performs no force; after the user switches to Workspaces, later launches restore Workspaces and never re-force Herdr. Also cover the ordinary fresh-config path with no prior selector interaction. Manual fresh-config launch check. |

Verify: `cargo test -p termy herdr`.
Safe stop: detection ships; the saved-preference path from P9 stays intact either way.

### P11. Settings UI rows (S1, S2)

| Task | Clauses | Work |
|---|---|---|
| T27 | S1, S2 | Rows for the enable toggle, which writes an explicit saved preference, the service path override, and the operator-editable trusted-paths list, all through the existing atomic setting writer. |
| T28 | S1 | Test: a toggled explicit preference wins over detection on the next resolve. |

Verify: `cargo test -p termy settings`.
Safe stop: operators control everything S1 and S2 name as operator-owned.

### P12. Control-conflict flow (S10)

| Task | Clauses | Work |
|---|---|---|
| T29 | S10 | Replace the P4 unavailable policy with the resolution flow: a conflict presents Take Over, Observe, and Cancel, with no tab and no input sink before an explicit choice. Observe constructs a tab carrying no writable binding, so read-only is structural rather than policed: the Observe attachment type lacks the input operation, the desktop input seams have nothing to write to, and no later code path can restore input without changing the type. Take Over requires its own confirmation and swaps in a writable binding. |
| T30 | S10 | Fake-transport tests: a conflict yields no tab and no input sink before resolution; an Observe tab carries no binding, and keystrokes and mouse packets on it reach neither the fake nor terminal input; Take Over becomes writable only after confirmation, after which input flows through the binding. Manual two-client exercise against the real Service. |

Verify: `cargo test -p termy_herdr_core conflict` and `cargo test -p termy herdr`.
Safe stop: contested attach is fully resolvable; the prior refusal behavior is gone.

### P13. Failure, empty, and reconnect states; preservation pass (S3, S11)

| Task | Clauses | Work |
|---|---|---|
| T31 | S2, S11 | Explanatory states: Service unavailable; untrusted location showing the rejected path; incompatible local service, where a handshake mismatch admits no wire value into the domain; lost connection as a marker that preserves the visible catalog and open-tab scrollback; and the empty-view state. Reconnect with backoff on the session thread, with reattach replaying idempotently. |
| T32 | S11 | Fake-transport drop-and-replay tests: catalog stays visible with the disconnected marker, reconnect replays idempotently, scrollback survives. |
| T33 | S11 | Manual kill-and-restart of the real Service: the view recovers automatically when the Service returns. |
| T34 | S3, S11 | Preservation pass: native tab create and close, Workspace switch and sidebar behavior, and tmux start and attach, each run with the integration disabled and enabled, on both runtimes. Existing suites stay green alongside the manual four-check pass. |

Verify: `cargo test -p termy_herdr_core reconnect`, `cargo test -p termy herdr`,
`cargo test -p termy workspaces`, plus the tmux launch tests.
Safe stop: complete feature per locked scope version 1.

## Direct verification

Every phase names commands the executing agent runs itself. The pass condition
is exit code 0 with the named tests green. Two checks are not command-shaped and
are performed against the real artifact: the manual end-to-end loop in T23 and
T33, and the preservation pass in T34.

Three verification levels back the plan:

1. **Unit against the fake transport.** The fake is the standing double the
   repository does not otherwise have. It covers trust rejection, catalog
   snapshots and revisions, create idempotency, the detach and close verbs, the
   conflict flow, and reconnect replay.
2. **Desktop state tests.** Sidebar exclusivity, dedup across active and stashed
   Workspaces, tmux rejection with zero mutation, quit detaching only,
   persist-and-restore round trips, and the probe-spy proof that a disabled
   startup does no Herdr work.
3. **One gated real-service integration test** plus the manual passes. The
   integration test skips when no Herdr binary is present, so it never turns CI
   red on a machine without one.

### Safe-stop criteria

Each phase ends in a state that can be left alone indefinitely:

- P1 to P8 are inert. The integration is unconfigured, so no Herdr code path
  runs and app behavior is upstream-identical.
- P9 is the first shippable state, complete for a user whose preference is
  already saved. Note that at P9 the preference can only be set by editing the
  config file, because the settings row that writes it is T27 in P11. The
  config is a user-editable text file, so this is a slice ordering choice and
  not a hole, but P9 is not "shippable to a user who has never opened the
  config file".
- P10 to P13 each add one scope clause and can each be the last phase landed
  without leaving a half-state, because the P9 behavior remains intact.

Any worker stops and records rather than widening when it hits a red gate it
cannot make green inside its phase, an edit needed outside its named files, a
scope question that maps to no clause, or the missing GM ticket.

## Acceptance map

| Acceptance | Covered by |
|---|---|
| Disabled does no Herdr work | P10 / T26 |
| Connect first, trusted start, untrusted rejected | P9 / T21, T22 and P10 / T24 |
| Live catalog in the Herdr view | P6 / T14, T15 |
| Exact program and argv | P7 / T16, T18 at the fake seam; P9 / T22 through the real transport |
| One tab per Agent per window, tmux gate | P7 / T17, T18 |
| Detach and reattach, including across relaunch | P8 / T19, T20; P9 / T22 at the transport boundary and T23 at the desktop boundary |
| Only confirmed Close Agent stops | P8 and P9 / T22 |
| Control conflict resolution | P12 / T29, T30 |
| Failure, empty, reconnect states | P13 / T31, T32, T33 |
| Ordinary Termy preserved on both runtimes | P13 / T34 |

## Independent verification

The plan was reviewed and then independently re-verified against live repository
source by a second reader who did not treat the first review as evidence. Result:
pass, with six non-blocking observations. The three that changed how tasks should
be read are folded into the task text above:

- The persistence exclusion in T20 is stated at pane granularity but must behave
  at tab granularity, because restore rejects a persisted tab with zero panes and
  because layout trees are built from the unfiltered pane list. T20 above now
  requires the round trip to succeed with layout trees intact.
- The Agent identity that T17 attaches to a tab has no home in P7's named file
  set if it is read as a new struct field. It belongs in the glue runtime's
  `AgentKey`-to-pane map, and both the input seams and persistence can
  discriminate on the terminal variant directly.
- The terminal grid accessor is an exhaustive `Terminal` policy that the task
  text did not enumerate. Its correct answer is delegation, not a none arm,
  because otherwise selection silently breaks on Agent tabs.

A post-review S4 correction is folded into T1, T2, T25, and T26. The locked
scope makes the selector visible while Herdr is disabled, so sidebar choice
cannot also represent whether the one-time Herdr-first presentation occurred.
The separate persisted presentation marker models those facts independently
without adding a phase or task.

One documentation-tier observation remains: two cited coordinates drift by one
line from the declaration they name. The claims are true; the anchors are one
off.
