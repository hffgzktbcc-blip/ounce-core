#!/usr/bin/env bash
set -euo pipefail

# Read non-secret config from environment -> state file -> fallback
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_FILE="${ORCA_SSH_STATE_FILE:-$SCRIPT_DIR/ssh-host-state.json}"

get_state() {
  local key="$1"
  if [ -f "$STATE_FILE" ]; then
    node -e '
      const fs = require("fs");
      try {
        const s = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
        const v = s[process.argv[2]];
        if (v !== undefined && v !== null) process.stdout.write(String(v));
      } catch (_) {}
    ' "$STATE_FILE" "$key"
  fi
}

host="${ORCA_SSH_HOST:-$(get_state host)}"
ssh_port="${ORCA_SSH_PORT:-$(get_state port)}"
ssh_username="${ORCA_SSH_USER:-$(get_state username)}"
identity_file="${ORCA_SSH_IDENTITY_FILE:-$(get_state identityFile)}"
jump_host="${ORCA_SSH_JUMP_HOST:-$(get_state jumpHost)}"
proxy_command="${ORCA_SSH_PROXY_COMMAND:-$(get_state proxyCommand)}"
project_root="${ORCA_PROJECT_ROOT:-$(get_state projectRoot)}"
repo_url="${ORCA_REPO_URL:-$(get_state repoUrl)}"
repo_ref="${ORCA_REPO_REF:-$(get_state repoRef)}"

: "${host:=127.0.0.1}"
: "${ssh_port:=22}"
: "${ssh_username:=$USER}"
: "${project_root:=/home/$ssh_username/workspace/ounce-core}"
: "${repo_url:=https://github.com/hffgzktbcc-blip/ounce-core.git}"
: "${repo_ref:=main}"
: "${identity_file:=}"
: "${jump_host:=}"
: "${proxy_command:=}"

if [ -n "$jump_host" ] && [ -n "$proxy_command" ]; then
  echo "set jump_host or proxy_command, not both" >&2
  exit 1
fi

ssh_opts=(-p "$ssh_port" -o BatchMode=yes -o StrictHostKeyChecking=yes)
[ -n "$identity_file" ] && ssh_opts+=(-i "$identity_file")
[ -n "$jump_host" ] && ssh_opts+=(-J "$jump_host")
[ -n "$proxy_command" ] && ssh_opts+=(-o "ProxyCommand=$proxy_command")

# If synchronization is enabled, update repo checkout on the remote host
if [ "${ORCA_SSH_SYNC:-0}" = "1" ]; then
  ssh_target="${ssh_username}@${host}"
  remote_sync='set -euo pipefail
    export GIT_TERMINAL_PROMPT=0
    [ -d "$project_root/.git" ] || git clone "$repo_url" "$project_root"
    cd "$project_root" && git fetch origin "$repo_ref" && git checkout -B "$repo_ref" FETCH_HEAD'
  ssh "${ssh_opts[@]}" "$ssh_target" "$(printf \
    'project_root=%q repo_url=%q repo_ref=%q bash -lc %q' \
    "$project_root" "$repo_url" "$repo_ref" "$remote_sync")" >&2
fi

# Print the SSH connection block on stdout (no stray echoes!)
node -e '
  const [host, port, user, idf, jh, pc, root] = process.argv.slice(1);
  const target = {
    label: "per-workspace-host",
    host,
    port: Number(port),
    username: user
  };
  if (idf) target.identityFile = idf;
  if (jh) target.jumpHost = jh;
  if (pc) target.proxyCommand = pc;
  const result = {
    schemaVersion: 1,
    connection: {
      type: "ssh",
      projectRoot: root,
      target
    }
  };
  process.stdout.write(JSON.stringify(result));
' "$host" "$ssh_port" "$ssh_username" "$identity_file" "$jump_host" "$proxy_command" "$project_root"
