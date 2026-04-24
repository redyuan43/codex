#!/usr/bin/env bash
set -euo pipefail

action="${1:-status}"
config_file="${2:-$HOME/.codex/config.toml}"

usage() {
  cat <<'EOF'
Usage:
  bash scripts/manage_codex_safe_no_approval.sh status [config.toml]
  bash scripts/manage_codex_safe_no_approval.sh apply [config.toml]
EOF
}

print_status() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    echo "CONFIG_MISSING $file"
    return 0
  fi

  awk '
    BEGIN {
      approval = "(unset)"
      sandbox = "(unset)"
    }
    /^\[/ { exit }
    /^approval_policy[[:space:]]*=/ {
      approval = $0
    }
    /^sandbox_mode[[:space:]]*=/ {
      sandbox = $0
    }
    END {
      print "CONFIG " FILENAME
      print approval
      print sandbox
    }
  ' "$file"
}

apply_mode() {
  local file="$1"
  local dir
  dir="$(dirname "$file")"
  mkdir -p "$dir"

  if [[ -f "$file" ]]; then
    local backup_file
    backup_file="$file.bak.$(date +%Y%m%d%H%M%S)"
    cp "$file" "$backup_file"
    echo "BACKUP $backup_file"
  fi

  local tmp_file
  tmp_file="$(mktemp)"

  if [[ -f "$file" ]]; then
    awk '
      BEGIN {
        inserted = 0
      }
      /^\[/ && inserted == 0 {
        print "approval_policy = \"never\""
        print "sandbox_mode = \"workspace-write\""
        print ""
        inserted = 1
      }
      inserted == 0 && /^approval_policy[[:space:]]*=/ {
        next
      }
      inserted == 0 && /^sandbox_mode[[:space:]]*=/ {
        next
      }
      {
        print
      }
      END {
        if (inserted == 0) {
          if (NR > 0) {
            print ""
          }
          print "approval_policy = \"never\""
          print "sandbox_mode = \"workspace-write\""
        }
      }
    ' "$file" >"$tmp_file"
  else
    cat >"$tmp_file" <<'EOF'
approval_policy = "never"
sandbox_mode = "workspace-write"
EOF
  fi

  mv "$tmp_file" "$file"
  echo "WROTE $file"
  print_status "$file"
}

case "$action" in
  status)
    print_status "$config_file"
    ;;
  apply)
    apply_mode "$config_file"
    ;;
  *)
    usage >&2
    exit 1
    ;;
esac
