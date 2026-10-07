#!/usr/bin/env bash
# install-wizard.sh — first-run / update installer for tmux-fingers-rs.
#
# Invoked by tmux-fingers-rs.tmux when the binary is missing or its version
# does not match the version declared in Cargo.toml. May also be run by
# the user directly with one of the action arguments below.
#
# Actions (passed as $1):
#   download-binary        download a prebuilt release binary from GitHub
#                          Releases (no Rust toolchain required)
#   install-from-crates    cargo install tmux-fingers-rs
#   install-from-source    cargo install --path "$CURRENT_DIR"
#   build-local            cargo build --release && copy binary to ./bin/
#   (none)                 show interactive tmux menu
#
# When invoked with no action it pops a `tmux display-menu` so the user
# can pick. When invoked with an action it does the work and reloads the
# plugin entrypoint on success.

set -u

CURRENT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
action="${1:-}"
tmpdir_to_clean=""
install_tmp_to_clean=""

function shell_quote() {
  printf "'%s'" "${1//\'/\'\\\'\'}"
}

function tmux_literal() {
  printf '%s' "${1//#/##}"
}

function tmux_quote() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//\$/\\\$}"
  printf '"%s"' "${value//#/##}"
}

# ---------- exit handling ---------------------------------------------------

function finish {
  exit_code=$?
  trap - EXIT

  [[ -z "$install_tmp_to_clean" ]] || rm -f "$install_tmp_to_clean" || exit_code=$?
  [[ -z "$tmpdir_to_clean" ]] || rm -rf "$tmpdir_to_clean" || exit_code=$?

  # Only intercept the exit code when there is an action defined.
  # Without an action we are just popping a menu; let it close cleanly.
  if [[ -z "$action" ]]; then
    exit "$exit_code"
  fi

  if [[ $exit_code -eq 0 ]]; then
    echo "Reloading tmux plugin..."
    # Unlike upstream, rerun the entrypoint so XDG and other tmux configs work.
    tmux run-shell "$(tmux_literal "$(shell_quote "$CURRENT_DIR/tmux-fingers-rs.tmux")")" || exit_code=$?
  fi

  if [[ $exit_code -eq 0 ]]; then
    echo
    echo "Done. Press any key to close this window."
    read -n 1 -r
    exit 0
  else
    echo
    echo "Something went wrong (exit $exit_code). Press any key to close this window."
    read -n 1 -r
    exit 1
  fi
}

trap finish EXIT

# ---------- helpers ---------------------------------------------------------

function require_cargo() {
  if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: \`cargo\` is not on \$PATH."
    echo
    echo "tmux-fingers-rs is distributed via crates.io and built with cargo."
    echo "Install Rust (which includes cargo) from:"
    echo
    echo "    https://rustup.rs"
    echo
    echo
    if [[ -n "$(detect_target)" ]]; then
      echo "Or pick \"Download prebuilt binary\" from the wizard menu to skip"
      echo "the Rust toolchain entirely."
      echo
    fi
    return 1
  fi
}

function require_curl_or_wget() {
  if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
  elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
  else
    echo "Error: neither \`curl\` nor \`wget\` is on \$PATH."
    return 1
  fi
}

function download_to() {
  local url="$1"
  local dest="$2"
  case "$DOWNLOADER" in
    curl) curl --fail --location --silent --show-error "$url" -o "$dest" ;;
    wget) wget --quiet --output-document="$dest" "$url" ;;
  esac
}

function expected_sha256() {
  local checksum="$1"
  local archive="$2"
  awk -v archive="$archive" '
    NR != 1 { exit 1 }
    NF != 2 || $2 != archive || length($1) != 64 || $1 ~ /[^0-9A-Fa-f]/ { exit 1 }
    { print tolower($1) }
    END { if (NR != 1) exit 1 }
  ' "$checksum"
}

function read_cargo_version() {
  if [[ ! -f "$CURRENT_DIR/Cargo.toml" ]]; then
    echo ""
    return
  fi
  grep -m1 '^version' "$CURRENT_DIR/Cargo.toml" \
    | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/'
}

function detect_target() {
  local sys mach
  sys="$(uname -s)"
  mach="$(uname -m)"
  case "$sys-$mach" in
    Linux-x86_64)  echo "x86_64-unknown-linux-gnu" ;;
    Darwin-arm64)  echo "aarch64-apple-darwin" ;;
    *) echo "" ;;
  esac
}

# ---------- actions ---------------------------------------------------------

function download_binary() {
  echo "Downloading prebuilt tmux-fingers-rs binary from GitHub Releases..."
  echo
  require_curl_or_wget || exit 1

  local target version tag base archive checksum tmpdir
  target="$(detect_target)"
  if [[ -z "$target" ]]; then
    echo "Error: no prebuilt binary is published for $(uname -s)/$(uname -m)."
    echo
    echo "Pick \"Install from crates.io\" or \"Build locally\" instead."
    exit 1
  fi

  version="$(read_cargo_version)"
  if [[ -z "$version" ]]; then
    echo "Error: could not read version from Cargo.toml."
    echo "This action expects to be run from inside a tmux-fingers-rs checkout."
    exit 1
  fi

  tag="v${version}"
  base="https://github.com/martintrojer/tmux-fingers-rs/releases/download/${tag}"
  archive="tmux-fingers-rs-${tag}-${target}.tar.gz"
  checksum="${archive}.sha256"
  tmpdir="$(mktemp -d)" || exit $?
  tmpdir_to_clean="$tmpdir"

  echo "Target:   $target"
  echo "Version:  $version"
  echo "URL:      $base/$archive"
  echo

  download_to "$base/$archive"  "$tmpdir/$archive" || exit $?
  download_to "$base/$checksum" "$tmpdir/$checksum" || exit $?

  echo "Verifying SHA256..."
  local expected actual
  expected="$(expected_sha256 "$tmpdir/$checksum" "$archive")" || exit $?
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmpdir/$archive")" || exit $?
  else
    actual="$(shasum -a 256 "$tmpdir/$archive")" || exit $?
  fi
  actual="${actual%%[[:space:]]*}"
  [[ "$actual" =~ ^[[:xdigit:]]{64}$ ]] || exit 1
  actual="$(printf '%s\n' "$actual" | tr '[:upper:]' '[:lower:]')" || exit $?
  [[ "$actual" == "$expected" ]] || exit 1

  echo "Extracting..."
  tar -C "$tmpdir" -xzf "$tmpdir/$archive" || exit $?

  mkdir -p "$CURRENT_DIR/bin" || exit $?
  install_tmp_to_clean="$(mktemp "$CURRENT_DIR/bin/.tmux-fingers-rs.XXXXXX")" || exit $?
  cp "$tmpdir/tmux-fingers-rs-${tag}-${target}/tmux-fingers-rs" \
     "$install_tmp_to_clean" || exit $?
  chmod a+x "$install_tmp_to_clean" || exit $?
  mv -f "$install_tmp_to_clean" "$CURRENT_DIR/bin/tmux-fingers-rs" || exit $?
  install_tmp_to_clean=""

  rm -rf "$tmpdir" || exit $?
  tmpdir_to_clean=""

  echo
  echo "Installed: $CURRENT_DIR/bin/tmux-fingers-rs"
  echo "The plugin entrypoint will pick it up automatically."
  exit 0
}

function install_from_crates() {
  echo "Installing tmux-fingers-rs from crates.io..."
  echo
  require_cargo || exit 1
  WIZARD_INSTALLATION_METHOD=cargo-install \
    cargo install --locked tmux-fingers-rs || exit $?
  if [[ -e "$CURRENT_DIR/bin/tmux-fingers-rs" ]]; then
    rm -f "$CURRENT_DIR/bin/tmux-fingers-rs" || exit $?
    echo "Removed the plugin-local binary so the new PATH installation is used."
  fi
  echo
  echo "Installed. Make sure ~/.cargo/bin is on your \$PATH."
  exit 0
}

function install_from_source() {
  echo "Installing tmux-fingers-rs from local checkout ($CURRENT_DIR)..."
  echo
  require_cargo || exit 1
  WIZARD_INSTALLATION_METHOD=cargo-install \
    cargo install --locked --path "$CURRENT_DIR" || exit $?
  if [[ -e "$CURRENT_DIR/bin/tmux-fingers-rs" ]]; then
    rm -f "$CURRENT_DIR/bin/tmux-fingers-rs" || exit $?
    echo "Removed the plugin-local binary so the new PATH installation is used."
  fi
  echo
  echo "Installed. Make sure ~/.cargo/bin is on your \$PATH."
  exit 0
}

function build_local() {
  echo "Building tmux-fingers-rs locally ($CURRENT_DIR)..."
  echo
  require_cargo || exit 1

  (cd "$CURRENT_DIR" && WIZARD_INSTALLATION_METHOD=build-from-source \
    cargo build --release) || exit $?

  mkdir -p "$CURRENT_DIR/bin" || exit $?
  install_tmp_to_clean="$(mktemp "$CURRENT_DIR/bin/.tmux-fingers-rs.XXXXXX")" || exit $?
  cp "$CURRENT_DIR/target/release/tmux-fingers-rs" "$install_tmp_to_clean" || exit $?
  chmod a+x "$install_tmp_to_clean" || exit $?
  mv -f "$install_tmp_to_clean" "$CURRENT_DIR/bin/tmux-fingers-rs" || exit $?
  install_tmp_to_clean=""

  echo
  echo "Built. Binary copied to: $CURRENT_DIR/bin/tmux-fingers-rs"
  echo "The plugin entrypoint will pick it up automatically."
  exit 0
}

# ---------- dispatch --------------------------------------------------------

case "$action" in
  download-binary)     download_binary     ;;
  install-from-crates) install_from_crates ;;
  install-from-source) install_from_source ;;
  build-local)         build_local         ;;
  "")                  : ;;  # fall through to menu
  *)
    echo "Unknown action: $action"
    echo "Valid actions: download-binary | install-from-crates | install-from-source | build-local"
    exit 2
    ;;
esac

# ---------- interactive menu ------------------------------------------------

function get_message() {
  if [[ "${FINGERS_UPDATE:-}" == "1" ]]; then
    echo "tmux-fingers-rs has been updated. Re-install to pick up the new version."
  else
    echo "First-time setup: install the tmux-fingers-rs binary."
  fi
}

wizard="$(shell_quote "$CURRENT_DIR/install-wizard.sh")"
menu_args=(
  -T "tmux-fingers-rs"
  ""
  "- " "" ""
  "-  #[nodim,bold]Welcome to tmux-fingers-rs ✌️ " "" ""
  "- " "" ""
  "-  $(get_message) " "" ""
  "- " "" ""
  ""
)

if [[ -n "$(detect_target)" ]]; then
  menu_args+=("Download prebuilt binary (recommended, no Rust required)" d "new-window $(tmux_quote "$wizard download-binary")")
fi

menu_args+=(
  "Install from crates.io (cargo install tmux-fingers-rs)" c "new-window $(tmux_quote "$wizard install-from-crates")"
  "Build locally into ./bin (TPM-friendly, no global install)" b "new-window $(tmux_quote "$wizard build-local")"
  "Install from this checkout (cargo install --path .)" s "new-window $(tmux_quote "$wizard install-from-source")"
  ""
  "Exit" q ""
)

tmux display-menu "${menu_args[@]}"
