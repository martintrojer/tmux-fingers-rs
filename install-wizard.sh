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
# can pick. When invoked with an action it does the work and re-sources
# ~/.tmux.conf on success.

set -u

CURRENT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
action="${1:-}"

# ---------- exit handling ---------------------------------------------------

function finish {
  exit_code=$?

  # Only intercept the exit code when there is an action defined.
  # Without an action we are just popping a menu; let it close cleanly.
  if [[ -z "$action" ]]; then
    exit $exit_code
  fi

  if [[ $exit_code -eq 0 ]]; then
    echo "Reloading tmux.conf..."
    tmux source ~/.tmux.conf 2>/dev/null || true
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
    echo "Or pick \"Download prebuilt binary\" from the wizard menu to skip"
    echo "the Rust toolchain entirely."
    echo
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
  tmpdir="$(mktemp -d)"

  echo "Target:   $target"
  echo "Version:  $version"
  echo "URL:      $base/$archive"
  echo

  download_to "$base/$archive"  "$tmpdir/$archive"
  download_to "$base/$checksum" "$tmpdir/$checksum"

  echo "Verifying SHA256..."
  pushd "$tmpdir" >/dev/null
    if command -v sha256sum >/dev/null 2>&1; then
      sha256sum --check "$checksum"
    else
      shasum -a 256 --check "$checksum"
    fi
  popd >/dev/null

  echo "Extracting..."
  tar -C "$tmpdir" -xzf "$tmpdir/$archive"

  mkdir -p "$CURRENT_DIR/bin"
  cp "$tmpdir/tmux-fingers-rs-${tag}-${target}/tmux-fingers-rs" \
     "$CURRENT_DIR/bin/tmux-fingers-rs"
  chmod a+x "$CURRENT_DIR/bin/tmux-fingers-rs"

  rm -rf "$tmpdir"

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
    cargo install --locked tmux-fingers-rs
  echo
  echo "Installed. Make sure ~/.cargo/bin is on your \$PATH."
  exit 0
}

function install_from_source() {
  echo "Installing tmux-fingers-rs from local checkout ($CURRENT_DIR)..."
  echo
  require_cargo || exit 1
  WIZARD_INSTALLATION_METHOD=cargo-install \
    cargo install --locked --path "$CURRENT_DIR"
  echo
  echo "Installed. Make sure ~/.cargo/bin is on your \$PATH."
  exit 0
}

function build_local() {
  echo "Building tmux-fingers-rs locally ($CURRENT_DIR)..."
  echo
  require_cargo || exit 1

  pushd "$CURRENT_DIR" > /dev/null
    WIZARD_INSTALLATION_METHOD=build-from-source \
      cargo build --release
  popd > /dev/null

  mkdir -p "$CURRENT_DIR/bin"
  cp "$CURRENT_DIR/target/release/tmux-fingers-rs" "$CURRENT_DIR/bin/tmux-fingers-rs"
  chmod a+x "$CURRENT_DIR/bin/tmux-fingers-rs"

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

tmux display-menu -T "tmux-fingers-rs" \
  "" \
  "- " "" "" \
  "-  #[nodim,bold]Welcome to tmux-fingers-rs ✌️ " "" "" \
  "- " "" "" \
  "-  $(get_message) " "" "" \
  "- " "" "" \
  "" \
  "Download prebuilt binary (recommended, no Rust required)" d "new-window \"$CURRENT_DIR/install-wizard.sh download-binary\"" \
  "Install from crates.io (cargo install tmux-fingers-rs)"   c "new-window \"$CURRENT_DIR/install-wizard.sh install-from-crates\"" \
  "Build locally into ./bin (TPM-friendly, no global install)" b "new-window \"$CURRENT_DIR/install-wizard.sh build-local\"" \
  "Install from this checkout (cargo install --path .)"      s "new-window \"$CURRENT_DIR/install-wizard.sh install-from-source\"" \
  "" \
  "Exit" q ""
