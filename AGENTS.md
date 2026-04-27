# AGENTS

## Scope

This folder contains the Rust port of `tmux-fingers`. Treat the Crystal implementation in the parent repo as the behavior reference unless there is a deliberate compatibility decision recorded in code or docs.

## Priorities

1. Preserve plugin-facing compatibility.
2. Prefer behavior parity over stylistic rewrites.
3. Keep tmux-dependent behavior covered by tests whenever practical.

## Important Contracts

The tmux plugin depends primarily on:
- `tmux-fingers version` returning the expected version string
- `tmux-fingers load-config` installing bindings and setting `@fingers-cli`
- `tmux-fingers start` working on real tmux panes
- `tmux-fingers send-input` remaining callable by bindings

Help formatting and `info` presentation are secondary unless they affect plugin flow.

## Working Rules

- Use the binary name `tmux-fingers`, not `tmux-fingers-rs`, for compatibility work.
- Keep `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` passing.
- Prefer extending existing live tmux tests over adding unverified runtime logic.
- When changing action execution or tmux command construction, assume socket handling matters.
- Do not weaken live tmux assertions to hide real runtime mismatches.

## Where To Look First

- CLI: `src/cli.rs`
- tmux wrapper: `src/tmux.rs`
- runtime flow: `src/fingers/start.rs`
- config loading and bindings: `src/fingers/load_config.rs`
- action execution: `src/fingers/action_runner.rs`
- live integration coverage: `tests/live_tmux.rs`

## Common Next Work

- Match Crystal `info` output more closely
- Add live coverage for `:open:`
- Add release/static build documentation
- Compare more directly against the installed Crystal binary when changing plugin-facing behavior
