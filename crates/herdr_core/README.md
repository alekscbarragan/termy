# termy_herdr_core

Pure Herdr integration domain and trust policy.

## Owner

This crate owns Herdr identities, lifecycle vocabulary, exact tokenized commands, confirmation evidence, trusted executable vetting, and the future transport and service-supervision boundary. T3–T5 implement only the pure domain and trust modules; desktop state and UI remain outside this crate.

## Validation

```sh
cargo test -p termy_herdr_core
```

## Forbidden Dependencies

- `gpui`
- `termy` / `crates/desktop_app`
- `termy_config_core`
- `termy_terminal_ui`
