# Gaps vs upstream `Morantron/tmux-fingers`

This is a hand-audited list of behavioral differences between this Rust
port and the upstream Crystal implementation, as of the most recently
ported upstream commit. It is not auto-generated; refresh it whenever
you port a batch of upstream changes.

**Audited against upstream `2.7.1`** (`fc3c750`). Upstream `2.6.2..2.7.1`
has been ported; see "Ported from 2.7.x" below for what that batch
covered and the deviations it introduced.

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
| Built-in regex patterns (`ip`, `uuid`, `sha`, `digit`, `url`, `path`, `hex`, `kubernetes`, `kubernetes-pod`, `git-status`, `git-status-branch`, `diff`) | ✅ identical (12 patterns, incl. 2.7.0's `kubernetes-pod`) |
| Style rendering (`TmuxStylePrinter`) | ✅ emits SGR sequences directly; no `tput` subprocess (upstream 2.7.0) |
| Config validation (`@fingers-*` values) | ⚠ stricter than upstream in two places (see below) |
| Action error reporting | ✅ failures are reported, not fatal (upstream 2.7.1) |
| Keyboard layouts (`qwerty`, `azerty`, `qwertz`, `dvorak`, `colemak`, plus `*-homerow` / `*-left-hand` / `*-right-hand` variants) | ✅ identical |
| Action semantics (`:copy:`, `:open:`, `:paste:`, custom shell actions) | ⚠ one platform-specific bug (see below) |
| Multi-mode | ✅ |
| Jump mode (cursor positioning via copy-mode) | ✅ |
| State preservation (`prefix`, `prefix2`, last key table, last pane) | ✅ |
| `Copied: ...` notification | ✅ |
| `info` command output format | ❌ different format and one missing field |
| `installation-method` reporting | ✅ set by every build path (release workflow + wizard actions) |
| WSL clipboard via `clip.exe` | 🚫 out of scope (this port does not target Windows / WSL) |
| `toggle-help` (bound to `?`) | ⚠ no-op in *both* implementations; not actually a gap |
| `fzf` action (bound to Space) | ⚠ no-op in *both* implementations; upstream comment says "soon" |

## Real gaps

### 1. `info` output is tab-separated text instead of an ASCII table

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

Upstream 2.7.x still reports `tmux-fingers <version>` here. This port
reports its own crate version and does **not** track upstream's version
number; the two projects version independently.

**Sub-gap:** the field is renamed `crystal-version` → `rust-version`,
but the value is hardcoded to `"unknown"`. Upstream reports the actual
Crystal compiler version. To restore parity we'd capture `rustc
--version` at build time via a `build.rs` that emits `cargo:rustc-env=`,
and read it back with `env!`. Not worth the build complexity yet.

## Out of scope

### WSL clipboard via `clip.exe`

**Symptom under upstream parity:** `system_copy_command_with` returns
`"clip.exe"` where upstream returns `"cat | clip.exe"`. Without the
`cat |` shell pipeline (and a shell to run it through), `clip.exe`
never receives the match on stdin and nothing ends up on the Windows
clipboard.

**Decision:** this port does not target Windows / WSL. The `clip.exe`
branch in `system_copy_command_with` is left intact for symmetry with
upstream, but it is not exercised, not fixed, and not tested. Linux
(`wl-copy`, `xclip`, `xsel`) and macOS (`pbcopy`,
`reattach-to-user-namespace`) clipboard backends are the supported set.

**Reconsider if:** a Windows / WSL user shows up and asks. The fix is
small (run the command through `sh -c` for the `clip.exe` arm, or
restructure to feed the match directly to `clip.exe`'s stdin without
the pipeline).

Note that upstream 2.7.0 (`38fe26b`, "fix clipboard integration in WSL")
changed its own `clip.exe` arm from `"cat | clip.exe"` to `"clip.exe"`.
This port already emitted plain `"clip.exe"`, so it now matches upstream
verbatim. The arm remains untested here.

## Ported from 2.7.x

The `2.6.2..2.7.1` batch landed as four commits. What each changed, and
where this port deliberately deviates:

### `kubernetes-pod` builtin pattern (upstream `9a0cff4` / `d52c6c0`)

Pattern string is byte-identical to upstream. Covered by unit tests in
`src/fingers/config.rs`.

### Style rendering (upstream `dabef36`, `a168542`, `e4280e1`, `8433fee`)

Upstream dropped `tput` and now emits `\e[38;5;Nm` / `\e[48;5;Nm`
directly; this port did the same and deleted its `Shell` abstraction.
Two `match_formatter` fixes came with it: a leading `reset_sequence` in
`format_offset`, and suppressing the backdrop prefix when a submatch
starts at offset 0.

Because the default styles are literal constants here rather than
computed by the printer at startup, `Config::default()` had to be
updated to the new SGR spelling (`\e[38;5;2m\e[1m`, not `\e[32m\e[1m`).
`config.rs::default_styles_match_the_style_printer` pins the constants
to the printer so the two cannot drift again.

### Action error reporting (upstream `7308c31`)

A failing action now prints a red full-width banner via
`display-message` and lets teardown run, instead of aborting. `start`
consequently exits **0** when only the action failed. This is a
user-visible behavior change from earlier versions of this port, and
`tests/live_tmux.rs::failed_action_is_reported_and_still_restores_tmux_state`
asserts the new contract.

### Config validation (upstream `b43b51f`, `fbc9ed8`, `1bcbcaa`)

Upstream replaced ad-hoc parsing with an options/parsers hierarchy that
validates enums, styles, actions and patterns up front. This port keeps
its flat `match` but performs the equivalent checks. Known deviations:

1. **Empty `@fingers-enabled-builtin-patterns` means "none".** Upstream's
   `MultiEnumParser` rejects `""` outright, so upstream offers no way to
   disable every builtin. We accept it. `tests/live_tmux.rs` relies on
   this to test user patterns in isolation.
2. **Whitespace around commas is trimmed.** `"ip, diff"` is a config
   error upstream and is accepted here. Only widens what loads.

Boolean options intentionally do **not** deviate: upstream's
`BoolParser` is `value == "1" || value.downcase == "true"` and declares
no `valid?`, so every value is accepted and non-truthy input is simply
false. This port matches that, including the `"true"` spelling.

Upstream's `1bcbcaa` (accept `pattern_foo` as well as `pattern-foo`) is a
no-op here: `option_to_method` already normalizes `-` to `_` before the
prefix is stripped.

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

### Version number

This port's `Cargo.toml` version tracks the port, not upstream. Porting
upstream 2.7.1 does **not** imply bumping this crate to 2.7.1.

### Test count

Upstream has ~45 spec cases; this port has 47 unit + 10 compliance + 7
live tmux tests (64 total). The Rust suite covers the same ground plus
the port-specific concerns (shell quoting, socket paths, teardown).

## Refreshing this document

After a porting session, re-walk the audit:

The last full re-walk was against `2.7.1` (`fc3c750`).

```sh
git fetch upstream
git checkout upstream-crystal && git merge --ff-only upstream/master

# Re-check the four surfaces:
# 1. CLI commands
git show upstream-crystal:src/fingers/cli.cr
ls $(git ls-tree --name-only upstream-crystal src/fingers/commands/)

# 2. Config keys — since 2.7.0 these live in options.cr, not config.cr
git show upstream-crystal:src/fingers/options.cr
ls $(git ls-tree --name-only upstream-crystal src/fingers/options/parsers/)

# 3. Built-in patterns — since 2.7.0 these live in constants.cr
git show upstream-crystal:src/fingers/constants.cr | grep -A 20 BUILTIN_PATTERNS

# 4. Tmux wrapper
git show upstream-crystal:src/tmux.cr | grep -E '^\s+def [a-z]'
```

Diff each against the corresponding Rust file under `src/` and update
this document.
