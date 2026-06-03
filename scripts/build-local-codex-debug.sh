#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
CODEX_RS_DIR="$REPO_ROOT/codex-rs"
DEBUG_BIN="$CODEX_RS_DIR/target/debug/codex"
ALIAS_NAME="${CODEX_LOCAL_ALIAS:-siyuan}"
ENSURE_ALIAS=false

usage() {
  cat <<EOF
Usage: scripts/build-local-codex-debug.sh [--ensure-siyuan-alias]

Build the local debug Codex CLI using a user-level rustup toolchain.

Options:
  --ensure-siyuan-alias  Add/update ~/.bashrc alias so '$ALIAS_NAME' points to the debug binary.
  --help, -h             Show this help.

Environment:
  CODEX_LOCAL_ALIAS      Alias name to verify/update. Default: siyuan.
EOF
}

step() {
  printf '==> %s\n' "$1"
}

warn() {
  printf 'WARNING: %s\n' "$1" >&2
}

die() {
  printf 'ERROR: %s\n' "$1" >&2
  exit 1
}

download_rustup_installer() {
  if command -v curl >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O - https://sh.rustup.rs
    return
  fi

  die "curl or wget is required to install rustup."
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --ensure-siyuan-alias)
        ENSURE_ALIAS=true
        ;;
      --help | -h)
        usage
        exit 0
        ;;
      *)
        die "Unknown argument: $1"
        ;;
    esac
    shift
  done
}

validate_alias_name() {
  case "$ALIAS_NAME" in
    *[!A-Za-z0-9_-]* | "")
      die "Invalid CODEX_LOCAL_ALIAS: $ALIAS_NAME"
      ;;
  esac
}

ensure_rustup() {
  if command -v rustup >/dev/null 2>&1; then
    return
  fi

  step "Installing user-level rustup"
  download_rustup_installer | RUSTUP_INIT_SKIP_PATH_CHECK=yes sh -s -- -y --default-toolchain stable --profile default
}

activate_cargo_path() {
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
}

ensure_repo_toolchain() {
  step "Preparing Rust toolchain from codex-rs/rust-toolchain.toml"
  cd "$CODEX_RS_DIR"
  rustup show active-toolchain >/dev/null
  cargo --version
  rustc --version
  cargo fmt --version >/dev/null
}

build_codex_cli() {
  step "Building codex-cli debug binary"
  cd "$CODEX_RS_DIR"
  cargo build -p codex-cli
}

verify_debug_binary() {
  step "Verifying debug binary"
  [ -x "$DEBUG_BIN" ] || die "Debug binary was not built: $DEBUG_BIN"
  ls -lh "$DEBUG_BIN"
  "$DEBUG_BIN" --version
}

ensure_bash_alias() {
  if [ "$ENSURE_ALIAS" != true ]; then
    return 0
  fi

  step "Ensuring ~/.bashrc alias for $ALIAS_NAME"
  BASHRC="$HOME/.bashrc"
  touch "$BASHRC"

  tmp_file=$(mktemp)
  # Remove old definitions for the selected alias, then append the desired one.
  grep -v -E "^alias[[:space:]]+$ALIAS_NAME=" "$BASHRC" >"$tmp_file" || true
  printf "alias %s='%s'\n" "$ALIAS_NAME" "$DEBUG_BIN" >>"$tmp_file"
  mv "$tmp_file" "$BASHRC"
}

verify_alias() {
  if command -v bash >/dev/null 2>&1; then
    step "Verifying interactive bash alias"
    bash -ic "type $ALIAS_NAME; $ALIAS_NAME --version" || warn "Alias '$ALIAS_NAME' is not available in interactive bash yet."
  fi
}

main() {
  parse_args "$@"
  validate_alias_name

  [ -d "$CODEX_RS_DIR" ] || die "Could not find codex-rs at $CODEX_RS_DIR"

  activate_cargo_path
  ensure_rustup
  activate_cargo_path
  ensure_repo_toolchain
  build_codex_cli
  verify_debug_binary
  ensure_bash_alias
  verify_alias

  step "Done"
}

main "$@"
