# tmux-fingers-rs

Rust port of [Morantron/tmux-fingers][upstream] (originally written in Crystal).

A tmux plugin that highlights items in a pane (paths, SHAs, IPs, UUIDs,
URLs, hex numbers, k8s resources, git status output, ...) with letter
hints. Type the letters to copy the match to the clipboard. Press
<kbd>prefix</kbd> + <kbd>F</kbd> to enter fingers mode.

The binary and plugin are named **`tmux-fingers-rs`** so they can coexist
on the same `$PATH` and in the same TPM configuration as the upstream
Crystal `tmux-fingers`.

[upstream]: https://github.com/Morantron/tmux-fingers

---

## Install

All install paths require [Rust / cargo](https://rustup.rs).

### Option A — TPM (recommended)

Add to your `~/.tmux.conf`:

```tmux
set -g @plugin 'martintrojer/tmux-fingers-rs'
```

Then <kbd>prefix</kbd> + <kbd>I</kbd> to fetch & source the plugin.

The first time it runs, a wizard pops up offering three install methods:

1. **Install from crates.io** — `cargo install tmux-fingers-rs` (puts the
   binary in `~/.cargo/bin/`; needs `~/.cargo/bin` on your `$PATH`).
2. **Build locally into `./bin`** — TPM-friendly, no global install. The
   binary stays inside `~/.tmux/plugins/tmux-fingers-rs/bin/` and the
   plugin script picks it up automatically.
3. **Install from this checkout** — `cargo install --path .` against the
   cloned plugin directory.

Pick whichever fits your setup. If you upgrade the plugin (TPM <kbd>U</kbd>)
and the binary version no longer matches `Cargo.toml`, the wizard pops up
again to rebuild — set `@fingers-skip-wizard 1` to suppress this.

### Option B — From crates.io directly

```sh
cargo install tmux-fingers-rs
```

Then add the plugin entrypoint to your `~/.tmux.conf` manually (or use
TPM as above; the binary on `$PATH` will be used and the wizard will
not run).

### Option C — Manual / from source

```sh
git clone https://github.com/martintrojer/tmux-fingers-rs ~/.tmux/plugins/tmux-fingers-rs
cd ~/.tmux/plugins/tmux-fingers-rs
cargo build --release
# the plugin script auto-discovers target/release/tmux-fingers-rs
```

Then in `~/.tmux.conf`:

```tmux
run-shell ~/.tmux/plugins/tmux-fingers-rs/tmux-fingers-rs.tmux
```

---

## Usage

While in fingers mode:

| Keys | Action |
| --- | --- |
| <kbd>a</kbd>–<kbd>z</kbd> | copy the matching match to the clipboard |
| <kbd>Ctrl</kbd>+<kbd>a</kbd>–<kbd>z</kbd> | copy and trigger the configured ctrl-action (default: `:open:`) |
| <kbd>Shift</kbd>+<kbd>a</kbd>–<kbd>z</kbd> | copy and trigger the shift-action (default: `:paste:`) |
| <kbd>Alt</kbd>+<kbd>a</kbd>–<kbd>z</kbd> | copy and trigger the alt-action (no default) |
| <kbd>Tab</kbd> | toggle multi-mode (select multiple matches) |
| <kbd>q</kbd> / <kbd>Esc</kbd> / <kbd>Ctrl</kbd>+<kbd>c</kbd> | exit |

Configuration uses tmux options of the form `@fingers-*`. The option
names are unchanged from upstream; see the
[upstream README](https://github.com/Morantron/tmux-fingers#configuration)
for the full list.

> **Compatibility note.** Some details still differ from the Crystal
> binary, notably `tmux-fingers-rs info` output formatting. File an
> issue if you hit a difference that breaks your workflow.

---

## Requirements

- tmux 3.0 or newer
- Rust 1.85+ (for `edition = "2024"`)

---

## Development

```sh
cargo build               # debug build       -> target/debug/tmux-fingers-rs
cargo build --release     # release build     -> target/release/tmux-fingers-rs
cargo test                # full test suite (incl. live tmux tests)
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
```

### Test suites

| Suite | What it covers | When |
| --- | --- | --- |
| `cargo test --lib` | unit tests (pure logic, command construction) | always |
| `cargo test --test compliance` | builtin regex parity with upstream | always |
| `cargo test --test live_tmux` | spawns real tmux servers, drives them via `-C`, asserts behavior of `start`, `paste`, `multi`, `jump`, custom patterns/actions | local only |

CI runs everything except `live_tmux` (it spawns long-lived tmux
processes and is best run interactively). Run it locally with:

```sh
cargo test --test live_tmux
```

### Useful one-liners

```sh
./target/debug/tmux-fingers-rs --help
./target/debug/tmux-fingers-rs start --help
./target/debug/tmux-fingers-rs version
./target/debug/tmux-fingers-rs info
```

---

## Tracking upstream

This repository is a Rust port of [Morantron/tmux-fingers][upstream].
Upstream Crystal changes are tracked on the `upstream-crystal` branch
and ported manually to `main`.

### Branches

| Branch | Purpose |
| --- | --- |
| `main` | This Rust port. Default branch. |
| `upstream-crystal` | Pristine fast-forward mirror of `Morantron/tmux-fingers` `master`. Never commit here. |

### One-time remote setup

```sh
git remote add upstream git@github.com:Morantron/tmux-fingers.git
git fetch upstream
```

### Refreshing `upstream-crystal`

Run periodically. It is fast-forward only — no local edits live on this
branch:

```sh
git fetch upstream
git checkout upstream-crystal
git merge --ff-only upstream/master
git push origin upstream-crystal
```

### Finding what needs porting

Port commits are recorded on `main` with a `Port:` subject prefix and
the upstream short SHA in parentheses, e.g.:

```
Port: fix line jumping with tabs/emojis in zoomed panes (upstream 99dafe6)
```

To list new upstream commits since the most recent port:

```sh
LAST=$(git log main --grep='^Port:' -n1 --pretty=format:%s \
        | grep -oE '[0-9a-f]{7,}' | head -1)
git log --oneline ${LAST}..upstream-crystal
```

Or simply browse:

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

This keeps the porting log greppable:

```sh
git log --oneline --grep='^Port:'
```

### Optional: tag porting checkpoints

After a session, snapshot how far you've caught up:

```sh
git tag ported/$(date +%Y-%m-%d) upstream-crystal
git push origin --tags
```

Then `git log ported/<date>..upstream-crystal` is always the unported
delta.

---

## License

MIT. See [LICENSE](./LICENSE). Original Crystal implementation © Jorge
Morante and contributors.
