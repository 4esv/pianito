# Contributing

This is a solo-maintainer project. Issues and small PRs are welcome; for
anything bigger, open an issue first so we don't duplicate effort.

## Build dependencies

- **macOS**: none beyond a Rust toolchain (uses CoreAudio via `cpal`).
- **Linux**: `libasound2-dev pkg-config` (ALSA headers for `cpal`).

## Before submitting a PR

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

All three must pass. CI runs the same gates on a macOS + Ubuntu matrix (plus
`cargo audit` and an MSRV check) and is the final arbiter if your local
toolchain lags behind.

## Commit style

Conventional commits (`type(scope): description`), e.g. `fix(audio): ...`.
