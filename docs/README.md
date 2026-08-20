# Contributor documentation

This directory contains repository-facing documentation. Public user guides
live under `website/content/`.

## Start here

- [Project layout](architecture/project-layout.md): ownership and dependency boundaries.
- [Development](development.md): crash logs, render metrics, benchmarks, and tmux testing.
- [Testing](engineering/testing.md): the test pyramid and smallest useful validation commands.
- [Release packaging](architecture/release-packaging.md): artifact and packaging ownership.

## Architecture

- [Command boundary](architecture/command-boundary.md)
- [Project layout](architecture/project-layout.md)
- [Release packaging](architecture/release-packaging.md)
- [SSH management](architecture/ssh-management.md)

## Engineering

The [engineering index](engineering/README.md) owns the quality roadmap,
scorecard, testing strategy, and decomposition plans.

## Plans

Scoped, reviewed planning packets for work that has not been implemented yet.

- [Herdr Integration: local Agent loop](plans/herdr-integration/README.md):
  optional fork-only Herdr Service integration. Planning complete; execution
  blocked on a GM ticket for its new production files.

## Reference documents

- [Configuration](configuration.md) — generated; do not edit directly.
- [Keybindings](keybindings.md) — generated; do not edit directly.
- [libtermy](libtermy.md)
- [Plugin runtime](plugins.md)

Repository benchmarks should be reproducible from the commands in
[Development](development.md); do not add standalone result snapshots without
the compared revisions, hardware, and measurement date.
