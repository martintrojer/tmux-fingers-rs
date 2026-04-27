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

### Option A — TPM (recommended)

Add to your `~/.tmux.conf`:

```tmux
set -g @plugin 'martintrojer/tmux-fingers-rs'
```

Then <kbd>prefix</kbd> + <kbd>I</kbd> to fetch & source the plugin.

The first time it runs, a wizard pops up offering four install methods:

1. **Download prebuilt binary** *(recommended, no Rust required)* —
   downloads the right release asset from GitHub, verifies its SHA256,
   and drops it in `~/.tmux/plugins/tmux-fingers-rs/bin/`. Available for
   Linux x86_64 and Apple Silicon macOS.
2. **Install from crates.io** — `cargo install tmux-fingers-rs` (puts the
   binary in `~/.cargo/bin/`; needs `~/.cargo/bin` on your `$PATH` and a
   Rust toolchain).
3. **Build locally into `./bin`** — TPM-friendly, no global install. The
   binary stays inside `~/.tmux/plugins/tmux-fingers-rs/bin/` and the
   plugin script picks it up automatically. Needs Rust.
4. **Install from this checkout** — `cargo install --path .` against the
   cloned plugin directory. Needs Rust.

Pick whichever fits your setup. If you upgrade the plugin (TPM <kbd>U</kbd>)
and the binary version no longer matches `Cargo.toml`, the wizard pops up
again to rebuild — set `@fingers-skip-wizard 1` to suppress this.

### Option B — Prebuilt binary, no plugin manager

Grab the appropriate `.tar.gz` from the
[latest release](https://github.com/martintrojer/tmux-fingers-rs/releases/latest):

- `tmux-fingers-rs-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` — Linux x86_64
- `tmux-fingers-rs-vX.Y.Z-aarch64-apple-darwin.tar.gz` — Apple Silicon macOS

Verify, extract, and put the binary on your `$PATH`:

```sh
tar -xzf tmux-fingers-rs-*.tar.gz
sudo install tmux-fingers-rs-*/tmux-fingers-rs /usr/local/bin/
```

Then in `~/.tmux.conf`:

```tmux
run-shell ~/path/to/tmux-fingers-rs/tmux-fingers-rs.tmux
```

or use TPM (Option A) and the plugin will pick the binary up from `$PATH`.

### Option C — From crates.io directly

Requires [Rust / cargo](https://rustup.rs):

```sh
cargo install tmux-fingers-rs
```

Then add the plugin entrypoint to your `~/.tmux.conf` manually (or use
TPM as above; the binary on `$PATH` will be used and the wizard will
not run).

### Option D — Manual / from source

Requires [Rust / cargo](https://rustup.rs):

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
- For prebuilt binaries: nothing else (Linux x86_64 or Apple Silicon macOS).
- For building from source / `cargo install`: Rust 1.95+ (pinned via
  `rust-toolchain.toml`).

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

## Releasing

Releases are driven by git tags. Pushing a tag of the form `vX.Y.Z`
triggers `.github/workflows/release.yml`, which:

- builds release binaries on `ubuntu-latest` (`x86_64-unknown-linux-gnu`)
  and `macos-latest` (`aarch64-apple-darwin`)
- packages each binary with `README.md`, `LICENSE`, the plugin script and
  the install wizard into a `.tar.gz` plus a `.sha256` sidecar
- creates the GitHub Release and uploads all artifacts

Publishing to crates.io is intentionally a separate manual step so that
it stays under explicit control:

```sh
# 0. Pre-flight
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib --bins --test compliance

# 1. Push main, wait for CI to go green
git push origin main

# 2. Dry-run the crates.io package
cargo publish --dry-run

# 3. Tag and push (this triggers the release workflow)
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0

# 4. Once the release workflow is green and the GitHub Release exists,
#    publish to crates.io. This is irrevocable.
cargo publish
```

If something goes wrong after `cargo publish`, you can `cargo yank
--version X.Y.Z` (hides the version from new resolves) and release a
patched `X.Y.Z+1`. You cannot re-upload the same version number.

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
