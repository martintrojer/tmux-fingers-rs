# Gaps vs upstream `Morantron/tmux-fingers`

This is a hand-audited list of behavioral differences between this Rust
port and the upstream Crystal implementation, as of the most recently
ported upstream commit. It is not auto-generated; refresh it whenever
you port a batch of upstream changes.

The methodology was: walk every public CLI command, every `@fingers-*`
config key, every `BUILTIN_PATTERNS` entry, every `tmux.rb`-equivalent
helper, and every spec in upstream `spec/`, and grep the Rust source to
confirm presence and equivalence.

## Summary

| Area | Status |
| --- | --- |
| CLI surface (`version`, `info`, `load-config`, `send-input`, `start`) | ✅ all 5 commands present |
| `start` flags (`--mode`, `--patterns`, `--main-action`, `--ctrl-action`, `--alt-action`, `--shift-action`) | ✅ identical |
| `@fingers-*` configuration keys | ✅ identical (21 keys) |
| Built-in regex patterns (`ip`, `uuid`, `sha`, `digit`, `url`, `path`, `hex`, `kubernetes`, `git-status`, `git-status-branch`, `diff`) | ✅ identical |
| Keyboard layouts (`qwerty`, `azerty`, `qwertz`, `dvorak`, `colemak`, plus `*-homerow` / `*-left-hand` / `*-right-hand` variants) | ✅ identical |
| Action semantics (`:copy:`, `:open:`, `:paste:`, custom shell actions) | ⚠ one platform-specific bug (see below) |
| Multi-mode | ✅ |
| Jump mode (cursor positioning via copy-mode) | ✅ |
| State preservation (`prefix`, `prefix2`, last key table, last pane) | ✅ |
| `Copied: ...` notification | ✅ |
| `info` command output format | ❌ different format and one missing field |
| `installation-method` reporting | ⚠ always reports `manual` (we don't set the build-time env yet) |
| `toggle-help` (bound to `?`) | ⚠ no-op in *both* implementations; not actually a gap |
| `fzf` action (bound to Space) | ⚠ no-op in *both* implementations; upstream comment says "soon" |

## Real gaps

### 1. WSL clipboard via `clip.exe` is missing the `cat |` prefix

**Where:** `src/fingers/action_runner.rs`, `system_copy_command_with`
returns `"clip.exe"` where upstream returns `"cat | clip.exe"`.

**Effect on Windows / WSL:** `clip.exe` reads from stdin, but our
caller invokes the command directly with no shell pipeline, so the
match never reaches `clip.exe`'s stdin and nothing ends up on the
Windows clipboard.

**Fix:** either return `"cat | clip.exe"` to match upstream and run the
command through a shell, or restructure the caller to feed the match
directly to `clip.exe`'s stdin (in which case the `cat |` is
unnecessary). Pick one.

**Severity:** breaks `:copy:` for WSL users. Has not affected anyone
yet because we have no WSL users.

### 2. `info` output is tab-separated text instead of an ASCII table

**Upstream** uses the `tablo` Crystal library to render a bordered
two-column table:

```
+--------------------+---------------------------------------------+
| Option             | Value                                       |
+--------------------+---------------------------------------------+
| tmux-fingers       | 2.6.2                                       |
| xdg-root-folder    | /home/user/.local/state/tmux-fingers        |
| log-path           | /home/user/.local/state/tmux-fingers/...    |
| installation-method| download-binary                             |
| tmux-version       | 3.4                                         |
| TERM               | tmux-256color                               |
| SHELL              | /bin/bash                                   |
| crystal-version    | 1.14.0                                      |
+--------------------+---------------------------------------------+
```

**This port** writes one tab-separated line per field:

```
tmux-fingers-rs	0.1.0
xdg-root-folder	/home/user/.local/state/tmux-fingers-rs
log-path	/home/user/.local/state/tmux-fingers-rs/fingers.log
installation-method	manual
tmux-version	3.4
TERM	tmux-256color
SHELL	/bin/bash
rust-version	unknown
```

**Severity:** cosmetic. Both formats are human-readable; ours is also
trivially machine-parseable (`cut -f`).

**Sub-gap:** the field is renamed `crystal-version` → `rust-version`,
but the value is hardcoded to `"unknown"`. Upstream reports the actual
Crystal compiler version. We should report `rustc --version` (set at
build time via `build.rs` and `env!`).

### 3. `installation-method` always reports `manual`

**Where:** `src/cli.rs`, the `Info` command reads
`option_env!("WIZARD_INSTALLATION_METHOD")` at compile time and falls
back to `"manual"`.

**Why:** none of our build paths (the release workflow, the install
wizard's `cargo build --release` and `cargo install --path .` actions)
set this env. Upstream's `install-wizard.sh` sets
`WIZARD_INSTALLATION_METHOD=build-from-source` for the
build-from-source action and `WIZARD_INSTALLATION_METHOD=download-binary`
for the deployment workflow.

**Fix:** set the env in `.github/workflows/release.yml`
(`download-binary`) and in `install-wizard.sh`'s `build_local` /
`install_from_source` actions (`build-from-source`). Optionally also in
the crates.io path (`cargo-install`).

**Severity:** trivial. Affects only the diagnostic output of `info`.

## Non-gaps (worth recording so we don't re-flag them)

### `toggle-help` is bound but does nothing

The `?` key is bound in fingers mode, but the dispatch in upstream
`view.cr` is:

```crystal
when "toggle-help"
  # (empty body)
```

The Rust port matches this exactly:

```rust
"toggle-help" | "fzf" | "noop" | "" => {}
```

There is no help overlay to port.

### `fzf` is bound to Space but does nothing

Upstream `view.cr`:

```crystal
when "fzf"
  # soon
```

Same story — bound, dispatches to a no-op. Not implemented in either
codebase.

### `Tmux` helpers `kill_window`, `resize_pane`, `set_window_option`, `zoom_pane`, `get_global_option`

Defined in upstream `src/tmux.cr` but **not called anywhere in upstream
`src/`**. They are dead code. The Rust port omits them, which is
correct.

### Spec-fixture configs (`spec/conf/*.conf`)

Upstream's `spec/conf/` files (`alt-action.conf`, `ctrl-action.conf`,
`custom-bindings.conf`, `custom-patterns.conf`, `invalid.conf`,
`quotes.conf`) are inputs for an end-to-end runner under
`spec/use-tmux.sh`. They are not unit tests.

The equivalent scenarios are covered by Rust:
- `invalid.conf` (`@fingers-lol`-style unknown options) → unit-tested
  in `src/fingers/load_config.rs::tests`.
- `custom-patterns.conf` → covered by
  `tests/live_tmux.rs::custom_pattern_is_loaded_and_selected`.
- `alt-action.conf` / `ctrl-action.conf` → covered by
  `tests/live_tmux.rs::custom_shell_action_receives_match_on_stdin`.
- `quotes.conf` (patterns containing quotes) → covered by
  `setup_bindings_quotes_cli_paths_with_spaces` and friends.

### Test count

Upstream has ~43 spec cases; this port has 70 unit + compliance + 7
live tmux tests (77 total). The Rust suite is broader, not narrower.

## Refreshing this document

After a porting session, re-walk the audit:

```sh
git fetch upstream
git checkout upstream-crystal && git merge --ff-only upstream/master

# Re-check the four surfaces:
# 1. CLI commands
git show upstream-crystal:src/fingers/cli.cr
ls $(git ls-tree --name-only upstream-crystal src/fingers/commands/)

# 2. Config keys
git show upstream-crystal:src/fingers/config.cr | grep -A 30 'def initialize'
git show upstream-crystal:src/fingers/commands/load_config.cr | grep '^\s*when'

# 3. Built-in patterns
git show upstream-crystal:src/fingers/config.cr | grep -A 20 BUILTIN_PATTERNS

# 4. Tmux wrapper
git show upstream-crystal:src/tmux.cr | grep -E '^\s+def [a-z]'
```

Diff each against the corresponding Rust file under `src/` and update
this document.
