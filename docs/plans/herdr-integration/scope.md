# Locked scope

Scope version 1, approved. Every task in
[implementation.md](implementation.md) cites one of the clauses below. The
clause labels S1-S11 are the numbering used by the plan and map in order to the
eleven in-scope statements in this document.

## Objective

A Termy user with the optional Herdr Integration enabled completes the entire
local Agent loop without leaving Termy. Termy finds or starts a trusted local
Herdr Service, the sidebar switches to a Herdr view listing local Spaces and
Agents with live status, the user creates an Agent, works in its dedicated tab,
detaches and reattaches freely, and stops the Agent only through an explicit
confirmed action.

On a fresh configuration the integration starts enabled only when a trusted
Herdr executable is found at an explicit absolute override or a documented
install location. A preference the user explicitly saves wins on later
launches. With the integration disabled, Termy performs no Herdr work.

## Terminology

| Term | Meaning |
|---|---|
| Termy Workspace | A persisted group of terminal tabs within Termy. Not a Herdr Space. |
| Herdr Space | A project context managed by Herdr that contains Herdr Agents. Not a Termy Workspace. |
| Sidebar View | One of the mutually exclusive contents of Termy's single left sidebar. |
| Herdr View | The Sidebar View that presents Herdr Spaces, Agents, statuses, and actions in Termy's visual language. |
| Agent Tab | A Native Runtime tab presenting a writable or read-only attachment to one Herdr Agent. At most one per Agent per app window. |
| Native Runtime | Termy's directly managed terminal-session mode. Agent Tabs require it. The optional tmux runtime stays available for ordinary work. |
| Herdr Service | The long-lived process that owns Spaces, Agents, and their terminal state. Termy may start it but never owns it, and it survives Termy quitting. |
| Take Over Agent | An explicit, separately confirmed action replacing another client as the writable controller of an Agent's terminal. |
| Close Agent | A destructive action that stops and removes an Agent and closes its tab. It is not the ordinary close-tab action. |
| Herdr Integration | The optional capability itself. Disabled means Termy behaves as the upstream terminal product. |

## In scope

**S1. One setting, detection only on a fresh configuration.** The integration is
controlled by a single setting. With no saved preference, it starts enabled only
when a trusted Herdr executable is found at an explicit absolute override or a
documented install location. An executable reachable only through inherited
PATH never triggers this and is never trusted. Once the user has explicitly
saved an enabled or disabled preference, that saved preference wins on later
launches. While disabled, Termy performs no Herdr work of any kind: no
detection, no process start, no connection.

**S2. Connect first, start only from a trusted absolute location.** When
enabled, Termy connects to an already-running local Herdr Service. Only if none
is running does it start one, and only from a trusted absolute filesystem
location: the bundled Service location plus operator-configured absolute paths,
editable only by the operator. An untrusted path (relative, PATH-resolved,
missing, non-executable, or outside the trusted set) is never launched, and the
rejected path is shown to the user. First-time start is silent, and the Herdr
view discloses that Termy started the Service.

**S3. One sidebar, a visible selector, two exclusive views.** Termy keeps one
left sidebar with a visible Workspaces / Herdr selector offering exactly two
mutually exclusive views: the Workspace view, unchanged from upstream, and the
Herdr view. Termy Workspaces and Herdr Spaces are never merged or cross-listed.

**S4. Herdr first exactly once, then view restore.** After the initial
auto-enable, the sidebar shows the Herdr view first exactly once. On later
launches, Termy restores the last selected sidebar view.

**S5. Live local catalog.** The Herdr view lists local Spaces and their Agents.
Each Agent shows a live status from the vocabulary `starting`, `running`,
`waiting-for-input`, `succeeded`, `failed`, updating while the view is open
without manual refresh. Writable-control ownership and connection loss appear as
separate markers, never as status values.

**S6. Exact program and argument list.** The user creates an Agent from a Space
row by giving an exact program and argument list. No shell-string interpretation
exists anywhere. A failed create leaves the form open with the error and adds no
Agent row. Space-row creation covers the multiple-local-Spaces case, so there is
no separate Space picker.

**S7. One Agent Tab per Agent per window, Native Runtime only.** Opening an
Agent yields that Agent's dedicated tab on the Native Runtime, at most one per
Agent per app window. A second open focuses the existing tab, including one
parked in an inactive Workspace. In tmux runtime, opening an Agent presents a
Native Runtime requirement and changes nothing.

**S8. Close and quit detach only.** Closing an Agent Tab, and quitting Termy,
detach only. The Agent keeps running with live status in the Herdr view.
Reopening the Agent reattaches to the same session, shows output accumulated
while detached, and continues live.

**S9. Stopping is a distinct confirmed action.** Stopping and removing an Agent
happens only through a distinct, confirmed Close Agent action, which also closes
its tab. The ordinary close-tab action never stops an Agent. Quitting Termy
never stops Agents or the Herdr Service, even when Termy started it.

**S10. Control conflicts are resolved explicitly.** Attaching to an Agent whose
writable control is owned by another client opens no tab and provides no input
path until the user explicitly chooses Take Over, Observe (read-only), or
Cancel. Take Over is fully functional in this slice and requires its own
confirmation.

**S11. Explanatory failure, empty, and reconnect states.** Service unavailable,
untrusted location, incompatible local service, lost connection, and an empty
Herdr view are each shown as a clear explanatory state. A lost connection
preserves the visible catalog and open-tab scrollback and recovers
automatically when the Service is reachable again.

## Preservation constraints

- Ordinary Termy behavior (tabs, Workspaces, tmux runtime sessions, existing
  sidebar behavior) works identically with the integration disabled and enabled.
  The change is fork-only and purely additive to upstream.
- With the integration disabled, ordinary behavior is unchanged from upstream.
  The only visible additions are the sidebar's Workspaces / Herdr selector and
  the integration setting itself.
- The Herdr Service and its Agents outlive Termy. No Termy action other than the
  explicit confirmed Close Agent stops an Agent, and nothing in Termy stops the
  Service.

## Out of scope

- Remote Herdr Devices, saved SSH host wiring, and any remote workflow,
  including remote image paste and its Herdr-owned staging.
- Herdr resource search through the command palette.
- Agent attention surfacing and Herdr notifications, system or in-app.
- Herdr capability version gating with upgrade explanations.
- Device and Space management beyond listing (creating or deleting Spaces,
  multi-Device handling).
- Agent Tabs on the tmux runtime.
- Agent Tab restart survival, meaning a tab re-establishing identity and
  position after app relaunch. After a restart the user reattaches from the
  Herdr view.
- A working-directory or target-Space input on Create Agent beyond the implicit
  Space row.

A related decision already recorded outside this slice: when remote Agent Tabs
eventually land, staging a pasted image on the target machine belongs to Herdr,
not to a second Termy transport. That decision constrains future work and
changes nothing here.

## Success criteria

1. With the integration disabled, no Herdr process is started or contacted
   during an entire app session, and ordinary tabs, Workspaces, and tmux
   sessions behave exactly as upstream.
2. On a fresh configuration, a trusted Herdr executable at an explicit absolute
   override or a documented install location yields the integration enabled with
   the Herdr view shown first exactly once. With no trusted executable found the
   integration starts disabled. An executable reachable only through inherited
   PATH never triggers auto-enable. After the user explicitly saves a preference,
   later launches honor it regardless of detection, and the sidebar restores the
   last selected view.
3. With the integration enabled and a Service already running, Termy connects
   without starting a second one. With none running, Termy starts one from a
   trusted location and the Herdr view lists that machine's Spaces and Agents.
   Pointed at an untrusted path, Termy launches nothing and shows the rejected
   path.
4. An Agent's status change appears in the open Herdr view untouched, and
   arguments containing spaces or shell metacharacters reach a created Agent
   verbatim as single arguments.
5. Opening a created Agent yields one Native Runtime tab. A second open focuses
   it, even parked in an inactive Workspace. In tmux runtime the user sees the
   requirement message and nothing changes.
6. Close-tab then reopen shows the output produced while detached and continues
   live. The same reattach works after quitting and relaunching Termy.
7. Only the confirmed Close Agent stops and removes the Agent. A control
   conflict presents Take Over / Observe / Cancel before any tab exists, Observe
   accepts no input, and Take Over becomes writable only after its confirmation.
8. A focused pass of ordinary-Termy checks (native tab create and close,
   Workspace switch and sidebar behavior, tmux start and attach) passes
   identically with the integration disabled and enabled, on both runtimes.
