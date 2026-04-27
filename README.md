# tmux-fingers (Rust)

Rust port of [Morantron/tmux-fingers][upstream] (originally written in Crystal),
intended to be a drop-in replacement for the Crystal binary used by the tmux
plugin.

[upstream]: https://github.com/Morantron/tmux-fingers

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

## Tracking upstream

This repository is a Rust port of the Crystal project
[Morantron/tmux-fingers][upstream]. Upstream changes are tracked on the
`upstream-crystal` branch and ported manually to `main`.

### Branches

| Branch             | Purpose                                                       |
| ------------------ | ------------------------------------------------------------- |
| `main`             | This Rust port. Default branch.                               |
| `upstream-crystal` | Pristine fast-forward mirror of `Morantron/tmux-fingers` `master`. Never commit here. |

### One-time remote setup

```sh
git remote add upstream git@github.com:Morantron/tmux-fingers.git
git fetch upstream
```

### Refreshing `upstream-crystal`

Run periodically. It is fast-forward only — no local edits live on this branch:

```sh
git fetch upstream
git checkout upstream-crystal
git merge --ff-only upstream/master
git push origin upstream-crystal
```

### Finding what needs porting

Port commits are recorded on `main` with a `Port:` subject prefix and the
upstream short SHA in parentheses, e.g.:

```
Port: fix line jumping with tabs/emojis in zoomed panes (upstream 99dafe6)
```

To list new upstream commits since the most recent port:

```sh
LAST=$(git log main --grep='^Port:' -n1 --pretty=format:%s \
        | grep -oE '[0-9a-f]{7,}' | head -1)
git log --oneline ${LAST}..upstream-crystal
```

Or review the full upstream history that post-dates this port's starting
point by browsing `upstream-crystal` directly:

```sh
git log --oneline upstream-crystal
```

### Porting a change

```sh
git checkout main
git checkout -b port/<short-description>

# Inspect the upstream change:
git show <upstream-sha>

# Implement the equivalent in Rust, with tests. Then:
git commit -m "Port: <upstream subject> (upstream <short-sha>)"
```

This convention keeps the porting log greppable:

```sh
git log --oneline --grep='^Port:'
```

### Optional: tag porting checkpoints

After a porting session, snapshot how far you've caught up:

```sh
git tag ported/$(date +%Y-%m-%d) upstream-crystal
git push origin --tags
```

Then `git log ported/<date>..upstream-crystal` is always the unported
delta.

### Things still to port from the Crystal repo

The following user-facing pieces are not yet present in this Rust
repository and currently live only on `upstream-crystal`. Port them when
you want this repo to be installable as a standalone tmux plugin (rather
than just a binary replacement consumed by the upstream plugin):

- `tmux-fingers.tmux` plugin entrypoint script (currently reads
  `shard.yml` for version comparison — needs to read `Cargo.toml` or
  `tmux-fingers version` instead)
- `install-wizard.sh`
- `docs/`, `CHANGELOG.md`, user-facing `README` content
- `.github/` issue and PR templates (currently still the Crystal ones)
