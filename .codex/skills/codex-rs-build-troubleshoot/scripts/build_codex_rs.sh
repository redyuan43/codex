#!/usr/bin/env bash
set -euo pipefail

action="${1:-status}"
repo_root="${2:-$HOME/github/codex/codex-rs}"
config_file="${CODEX_CONFIG_FILE:-$HOME/.codex/config.toml}"

usage() {
  cat <<'EOS'
Usage:
  bash scripts/build_codex_rs.sh status [repo_root]
  bash scripts/build_codex_rs.sh sync [repo_root]
  bash scripts/build_codex_rs.sh build [repo_root]
  bash scripts/build_codex_rs.sh check-binary [repo_root]
  bash scripts/build_codex_rs.sh doctor-loop [repo_root]
  bash scripts/build_codex_rs.sh tests [repo_root]
EOS
}

require_repo() {
  if ! git -C "$repo_root" rev-parse --show-toplevel >/dev/null 2>&1; then
    echo "REPO_MISSING $repo_root" >&2
    exit 1
  fi
}

print_header() {
  echo "REPO $repo_root"
  printf 'BRANCH %s\n' "$(git -C "$repo_root" branch --show-current)"
}

status() {
  require_repo
  print_header
  git -C "$repo_root" status --short
}

sync_repo() {
  require_repo
  print_header
  git -C "$repo_root" pull --ff-only
}

build_cli() {
  require_repo
  print_header
  (
    cd "$repo_root"
    cargo build -p codex-cli
  )
}

check_binary() {
  require_repo
  print_header
  local target_bin="$repo_root/target/debug/codex"
  printf 'WHICH_CODEX %s\n' "$(command -v codex || echo '(not found)')"
  if command -v codex >/dev/null 2>&1; then
    printf 'WHICH_CODEX_REAL %s\n' "$(readlink -f "$(command -v codex)" 2>/dev/null || command -v codex)"
    codex --version || true
  fi
  if [[ -x "$target_bin" ]]; then
    printf 'TARGET_DEBUG_CODEX %s\n' "$target_bin"
    stat -c 'TARGET_DEBUG_CODEX_MTIME %y' "$target_bin"
    "$target_bin" --version || true
  else
    printf 'TARGET_DEBUG_CODEX %s\n' "(missing)"
  fi
}

doctor_loop() {
  require_repo
  print_header
  local feature_file="$repo_root/features/src/lib.rs"
  local target_bin="$repo_root/target/debug/codex"

  echo "CONFIG_FILE $config_file"
  if [[ -f "$config_file" ]]; then
    awk '
      /^\[features\]/ { in_features = 1; print; next }
      /^\[/ && in_features == 1 { exit }
      in_features == 1 && /^alarm_tool[[:space:]]*=/ { print }
    ' "$config_file"
  else
    echo "CONFIG_MISSING"
  fi

  echo "---"
  if [[ -f "$feature_file" ]]; then
    grep -n -A4 -B1 'Feature::AlarmScheduler\|key: "alarm_tool"' "$feature_file" || true
    stat -c 'FEATURE_FILE_MTIME %y' "$feature_file"
  else
    echo "FEATURE_FILE_MISSING"
  fi

  echo "---"
  if [[ -x "$target_bin" ]]; then
    stat -c 'TARGET_DEBUG_CODEX_MTIME %y' "$target_bin"
  else
    echo "TARGET_DEBUG_CODEX_MISSING"
  fi

  echo "---"
  check_binary
}

run_tests() {
  require_repo
  print_header
  (
    cd "$repo_root"
    cargo test -p codex-tui
    cargo test -p codex-app-server-protocol
  )
}

case "$action" in
  status)
    status
    ;;
  sync)
    sync_repo
    ;;
  build)
    build_cli
    ;;
  check-binary)
    check_binary
    ;;
  doctor-loop)
    doctor_loop
    ;;
  tests)
    run_tests
    ;;
  *)
    usage >&2
    exit 1
    ;;
esac
