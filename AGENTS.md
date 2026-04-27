# AGENTS

Notes for AI / automation agents working in this repository.

## Scope

This is the Rust port of [Morantron/tmux-fingers][upstream]. Treat the
upstream Crystal implementation as the behavior reference unless there
is a deliberate compatibility decision recorded in code or docs. The
upstream sources are available on the `upstream-crystal` branch of this
repo (see the README's "Tracking upstream" section).

[upstream]: https://github.com/Morantron/tmux-fingers

## Priorities

1. Preserve plugin-facing behavior (matching, hint assignment, action
   semantics) so users migrating from upstream are not surprised.
2. Prefer behavior parity with upstream over stylistic rewrites.
3. Keep tmux-dependent behavior covered by tests whenever practical.

## Naming

- Binary name: **`tmux-fingers-rs`** (so it can coexist with upstream's
  `tmux-fingers` on the same `$PATH` and TPM config).
- Plugin entrypoint: `tmux-fingers-rs.tmux`.
- Crate: `tmux-fingers-rs` (published to crates.io).
- The string `tmux-fingers` (without `-rs`) should appear only when
  referring to the upstream project, in legitimate test fixtures, or in
  internal log/error prefixes that have already been updated to
  `[tmux-fingers-rs]`.

## Important contracts

The plugin script (`tmux-fingers-rs.tmux`) depends on:

- `tmux-fingers-rs version` returning a version string that matches
  the `version = ` line in `Cargo.toml`.
- `tmux-fingers-rs load-config` installing bindings and setting
  `@fingers-cli`.
- `tmux-fingers-rs start <pane-id>` working against a real tmux pane.
- The hidden `tmux-fingers-rs send-input` remaining callable by
  generated bindings.

Help formatting and `info` presentation are secondary unless they affect
plugin flow.

## Working rules

- Keep `cargo fmt --all -- --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, and `cargo test --lib --bins --test
  compliance` green. CI runs all three.
- The `live_tmux` integration suite spawns real tmux servers; it is not
  in CI and should be exercised locally before behavior-affecting
  changes.
- Prefer extending existing live tmux tests over adding unverified
  runtime logic.
- When changing action execution or tmux command construction, assume
  socket handling matters (see `src/fingers/input_socket.rs` and the
  socket-path notes in `tests/live_tmux.rs`).
- Do not weaken live tmux assertions to hide real runtime mismatches.

## Where to look first

- CLI:                       `src/cli.rs`
- tmux wrapper:              `src/tmux.rs`
- runtime flow:              `src/fingers/start.rs`
- config + bindings:         `src/fingers/load_config.rs`
- action execution:          `src/fingers/action_runner.rs`
- live integration coverage: `tests/live_tmux.rs`
- plugin entrypoint:         `tmux-fingers-rs.tmux`
- installer:                 `install-wizard.sh`

## Porting workflow

When upstream Crystal changes, port them on a branch off `main` with a
commit subject of the form:

```
Port: <upstream subject> (upstream <short-sha>)
```

The full workflow is documented in the README under "Tracking upstream".

## Common next work

- Match upstream `info` output more closely.
- Add live coverage for `:open:`.
- Document release/static-build process for `cargo install` and TPM
  consumers.
- Add a release workflow under `.github/workflows/` (currently CI-only).
