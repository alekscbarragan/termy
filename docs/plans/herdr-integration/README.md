# Herdr Integration: local Agent loop

Planning packet for an optional, fork-only Termy capability that connects to a
local Herdr Service and lets a user run the whole local Agent loop without
leaving Termy.

## Status

| Field | Value |
|---|---|
| Stage | Planning complete, reviewed, independently verified |
| Implementation | None. No production code exists for this feature |
| Scope | Locked at version 1 and approved |
| Plan | 13 phases (P1-P13), 34 tasks (T1-T34), reviewed; the repository packet includes the required post-review S4 persistence correction |
| Execution | **Blocked.** Requires a real GM ticket for new production files NF1-NF3 |

Independent verification found that a saved sidebar choice made while Herdr was disabled could suppress the required one-time Herdr-first presentation. This packet corrects T1, T2, T25, and T26 with a distinct persisted presentation marker. The task inventory is unchanged.

This directory contains documentation only. Nothing here has been implemented.
No crate, module, setting, or test described in this packet exists in the tree.
Work cannot start until a maintainer supplies a real GM ticket that authorizes
the three new production file groups listed under
[Gates](implementation.md#execution-gates). No ticket ID is invented here, and
none of the gated logic may be relocated into existing files to get around the
gate.

## Contents

| Document | Purpose |
|---|---|
| [scope.md](scope.md) | The locked behavior: objective, clauses S1-S11, preservation constraints, non-goals, success criteria |
| [architecture.md](architecture.md) | Module ownership, the pure `herdr_core` boundary, desktop glue, seams into existing files, invariants, failure behavior |
| [implementation.md](implementation.md) | Phase and task roadmap P1-P13 / T1-T34, gates, verification, safe stops |
| [report.html](report.html) | Single-page decision report for review; open it from disk, no server needed |

## Terminology

Two words in this packet look interchangeable and are not.

- **Termy Workspace**: a persisted group of terminal tabs inside Termy.
- **Herdr Space**: a project context managed by Herdr that contains Herdr Agents.

They are never merged, cross-listed, or shown in the same list. The sidebar
holds exactly two mutually exclusive views: the Workspace view (unchanged from
upstream) and the Herdr view. Full vocabulary is in
[scope.md](scope.md#terminology).

## What the feature does

With the integration enabled, Termy finds or starts a trusted local Herdr
Service, shows a Herdr view listing that machine's Spaces and Agents with live
status, creates Agents from a Space row using an exact program and argument
list, opens one Agent Tab per Agent per window on the Native Runtime, detaches
on ordinary tab close and on quit, reattaches with the output produced while
detached, and stops an Agent only through a distinct confirmed Close Agent
action.

With the integration disabled, Termy performs no Herdr work of any kind and
behaves exactly as upstream.

## Next action

A maintainer files a GM ticket covering NF1-NF3. Until then the packet stays as
reference material and the only runnable work is the two ungated phases
described in [implementation.md](implementation.md#phase-roadmap).
