#!/bin/sh
# narrater — the NarraTer CLI in a remote workspace (SSH server, Docker
# container). It only carries the command line to the app's bridge — through
# the SSH reverse tunnel or the container's gateway — and prints the answer;
# the app parses and runs it. Needs sh and curl.

bridge=${NARRATER_BRIDGE:-}
token=${NARRATER_TOKEN:-}
# A persistent session (tmux) outlives the connection that started it: the
# current bridge and token are published in tmux's global environment under
# this terminal's key, so prefer them when present.
if [ -n "${TMUX:-}" ] && [ -n "${NARRATER_KEY:-}" ] && command -v tmux >/dev/null 2>&1; then
  b=$(tmux show-environment -g "NARRATER_BRIDGE_$NARRATER_KEY" 2>/dev/null | cut -d= -f2-)
  t=$(tmux show-environment -g "NARRATER_TOKEN_$NARRATER_KEY" 2>/dev/null | cut -d= -f2-)
  [ -n "$b" ] && bridge=$b
  [ -n "$t" ] && token=$t
fi

diagnose() {
  echo "=== NarraTer CLI Diagnostics (remote) ==="
  echo
  echo "--- Environment ---"
  for k in NARRATER_ID NARRATER_LABEL NARRATER_SCENE NARRATER_KEY NARRATER_CLI; do
    eval "v=\${$k:-(not set)}"
    printf '%-16s %s\n' "$k" "$v"
  done
  printf '%-16s %s\n' NARRATER_BRIDGE "${bridge:-(not set)}"
  printf '%-16s %s\n' NARRATER_TOKEN "$([ -n "$token" ] && echo '(set)' || echo '(not set)')"
  printf '%-16s %s\n' CLI "$0"
}

tmp=$(mktemp "${TMPDIR:-/tmp}/narrater.XXXXXX") || exit 1
out="$tmp.out"
trap 'rm -f "$tmp" "$out"' EXIT

argc=$#
for a in "$@"; do printf '%s\0' "$a"; done > "$tmp"

# `say` without text reads its answer from stdin (heredocs)
stdin=0
if [ "${1:-}" = say ] && [ ! -t 0 ]; then
  need=1; skip=0; first=1
  for a in "$@"; do
    if [ $first = 1 ]; then first=0; continue; fi
    if [ $skip = 1 ]; then skip=0; continue; fi
    case "$a" in
      --attach) skip=1 ;;
      --file) need=0; skip=1 ;;
      --progress) ;;
      *) need=0 ;;
    esac
  done
  if [ $need = 1 ]; then cat >> "$tmp"; stdin=1; fi
fi

first=${1:-}
case "$bridge" in
  unix:*) set -- --unix-socket "${bridge#unix:}" "http://narrater/cli" ;;
  tcp:*) set -- "http://${bridge#tcp:}/cli" ;;
  *)
    if [ "$first" = debug ]; then diagnose; echo; echo "Bridge:          NARRATER_BRIDGE not set — not inside a NarraTer terminal?"; exit 1; fi
    echo "Error: NARRATER_BRIDGE not set. Run this from a NarraTer terminal (\`narrater debug\` to diagnose)." >&2
    exit 1 ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: curl is not installed here — the narrater CLI needs it in remote workspaces." >&2
  exit 1
fi

code=$(curl -sS -o "$out" -w '%{http_code}' -X POST --data-binary @"$tmp" \
  -H "Content-Type: application/octet-stream" \
  -H "X-Narrater-Id: ${NARRATER_ID:-}" \
  -H "X-Narrater-Token: $token" \
  -H "X-Narrater-Argc: $argc" \
  -H "X-Narrater-Stdin: $stdin" \
  -H "X-Narrater-Home: ${HOME:-}" \
  -H "X-Narrater-Cwd: $PWD" \
  "$@")
status=$?
if [ $status -ne 0 ]; then
  if [ "$first" = debug ]; then diagnose; echo; echo "Bridge:          unreachable ($bridge) — restart this terminal from the canvas."; exit 1; fi
  echo "Error: cannot reach NarraTer through its bridge ($bridge). Run \`narrater debug\`." >&2
  exit 1
fi

case "$code" in
  200) cat "$out" ;;
  201) cat "$out"; exit 1 ;;
  *) cat "$out" >&2; exit 1 ;;
esac
