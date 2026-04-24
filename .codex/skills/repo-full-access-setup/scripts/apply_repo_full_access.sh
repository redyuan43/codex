#!/usr/bin/env bash
set -euo pipefail

target="${1:-$PWD}"

if [[ ! -d "$target" ]]; then
  echo "Target directory not found: $target" >&2
  exit 1
fi

target="$(cd "$target" && pwd -P)"
codex_dir="$target/.codex"
config_file="$codex_dir/config.toml"

mkdir -p "$codex_dir"

if [[ -f "$config_file" ]]; then
  backup_file="$config_file.bak.$(date +%Y%m%d%H%M%S)"
  cp "$config_file" "$backup_file"
  echo "BACKUP $backup_file"
fi

cat >"$config_file" <<'EOF'
approval_policy = "never"
sandbox_mode = "danger-full-access"
allow_login_shell = false
EOF

echo "WROTE $config_file"
