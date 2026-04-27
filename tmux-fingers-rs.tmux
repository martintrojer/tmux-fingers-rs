#!/usr/bin/env bash
# tmux-fingers-rs — tmux plugin entrypoint for the Rust port of tmux-fingers.
#
# Resolution order for the binary:
#   1. `tmux-fingers-rs` on $PATH
#   2. <plugin dir>/bin/tmux-fingers-rs           (placed by install-wizard.sh)
#   3. <plugin dir>/target/release/tmux-fingers-rs
#   4. <plugin dir>/target/debug/tmux-fingers-rs  (handy during development)
#
# If no binary is found, the install wizard is launched.
# If a binary is found but its `version` output disagrees with the version
# declared in Cargo.toml, the wizard is launched in update mode (unless
# @fingers-skip-wizard is set).
#
# Note: this plugin and binary are deliberately named `tmux-fingers-rs` so
# they can coexist with the upstream Crystal `tmux-fingers` on the same $PATH
# and in the same TPM configuration.

set -u

CURRENT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

FINGERS_BINARY=""
if command -v tmux-fingers-rs &>/dev/null; then
  FINGERS_BINARY="tmux-fingers-rs"
elif [[ -x "$CURRENT_DIR/bin/tmux-fingers-rs" ]]; then
  FINGERS_BINARY="$CURRENT_DIR/bin/tmux-fingers-rs"
elif [[ -x "$CURRENT_DIR/target/release/tmux-fingers-rs" ]]; then
  FINGERS_BINARY="$CURRENT_DIR/target/release/tmux-fingers-rs"
elif [[ -x "$CURRENT_DIR/target/debug/tmux-fingers-rs" ]]; then
  FINGERS_BINARY="$CURRENT_DIR/target/debug/tmux-fingers-rs"
fi

if [[ -z "$FINGERS_BINARY" ]]; then
  tmux run-shell -b "bash $CURRENT_DIR/install-wizard.sh"
  exit 0
fi

CURRENT_FINGERS_VERSION="$($FINGERS_BINARY version 2>/dev/null || true)"

# Read the version from Cargo.toml. Match the first line that looks like:
#   version = "x.y.z"
CARGO_TOML="$CURRENT_DIR/Cargo.toml"
if [[ -f "$CARGO_TOML" ]]; then
  CURRENT_GIT_VERSION="$(grep -m1 '^version' "$CARGO_TOML" \
    | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
else
  CURRENT_GIT_VERSION=""
fi

SKIP_WIZARD="$(tmux show-option -gqv @fingers-skip-wizard)"
SKIP_WIZARD="${SKIP_WIZARD:-0}"

if [[ "$SKIP_WIZARD" = "0" \
      && -n "$CURRENT_GIT_VERSION" \
      && -n "$CURRENT_FINGERS_VERSION" \
      && "$CURRENT_FINGERS_VERSION" != "$CURRENT_GIT_VERSION" ]]; then
  tmux run-shell -b "FINGERS_UPDATE=1 bash $CURRENT_DIR/install-wizard.sh"
  if [[ "$?" != "0" ]]; then
    echo "Something went wrong while updating tmux-fingers-rs. Please try again."
    exit 1
  fi
fi

if [[ "${TERM:-}" == "dumb" ]]; then
  # force a real $TERM value to get proper colors in systemd and tmux 3.6a
  # https://github.com/Morantron/tmux-fingers/issues/143
  FINGERS_TERM="$(tmux show-option -gqv default-terminal)"
else
  FINGERS_TERM="${TERM:-}"
fi

tmux run "TERM=$FINGERS_TERM $FINGERS_BINARY load-config"
exit $?
