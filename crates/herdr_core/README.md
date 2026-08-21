# termy_herdr_core

Pure Herdr integration domain, trust policy, transport seam, and catalog mirror.

## Owner

This crate owns Herdr identities, lifecycle vocabulary, exact tokenized commands, confirmation evidence, trusted executable vetting, the private transport seam, the derived catalog mirror, and controller policy for agent mutations and writable attachments. Service supervision is deferred to a later phase; desktop state and UI remain outside this crate.

## Validation

```sh
cargo test -p termy_herdr_core
```

## Forbidden Dependencies

- `gpui`
- `termy` / `crates/desktop_app`
- `termy_config_core`
- `termy_terminal_ui`
