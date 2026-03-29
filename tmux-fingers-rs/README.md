# tmux-fingers-rs

Rust port of `tmux-fingers`, intended to be a drop-in replacement for the Crystal binary used by the tmux plugin.

## Install

Install the binary from this directory:

```sh
cargo install --path .
```

The tmux plugin checks for `tmux-fingers` on your `PATH` first, so after installation it will pick up this binary automatically.

## Production Build

For a release build without installing it:

```sh
cargo build --release
```

Binary path:

```sh
./target/release/tmux-fingers
```

Current status:
- Binary target name is `tmux-fingers`
- Version is aligned to `2.6.2`
- `load-config`, `start`, `send-input`, `version`, and `info` exist
- Runtime behavior is covered by unit, compliance, and live tmux tests

## Build

```sh
cargo build
```

Debug binary:

```sh
./target/debug/tmux-fingers
```

## Test

Run everything:

```sh
cargo test
```

Strict lint:

```sh
cargo clippy --all-targets --all-features -- -D warnings
```

Format:

```sh
cargo fmt
```

## Test Coverage

The suite currently includes:
- unit tests for pure logic and command construction
- compliance tests for builtin regex behavior
- live tmux integration tests for:
  - default selection
  - multimode
  - jump mode
  - custom patterns
  - `:paste:`
  - custom shell actions

## Compatibility Notes

The main compatibility target is the tmux plugin in the parent repo. The most important contracts are:
- `tmux-fingers version`
- `tmux-fingers load-config`
- `tmux-fingers start`
- hidden `tmux-fingers send-input`

Some presentation details still differ from the Crystal binary, especially `info` output formatting.

## Useful Commands

Compare local binary help:

```sh
./target/debug/tmux-fingers --help
./target/debug/tmux-fingers start --help
```

Run only live tmux tests:

```sh
cargo test --test live_tmux
```
