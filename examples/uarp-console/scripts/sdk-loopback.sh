#!/bin/sh
# Runs the real uarp-sdk client under Varta against a loopback mock API that also
# runs under Varta, as two `varta dev` processes:
#   VARTA_BIN=/path/to/varta scripts/sdk-loopback.sh [group…]
# 1. The mock (`varta.sdk-server.json`, src/tests/sdk-server/) binds 127.0.0.1 on an
#    OS-assigned port (exact grant 127.0.0.1:0) and reports `varta listening: <url>`.
# 2. The client config `varta.sdk-test.json` grants `net.request` on a placeholder
#    origin; the run writes a copy with the bound origin (an exact grant: ports are
#    never wildcards) next to it as `varta.sdk-test.loopback-<pid>.json`, removed on
#    exit. The name is git-ignored (.gitignore). A run killed without its exit trap
#    (SIGKILL, a crashed shell) leaves that file and possibly the mock process
#    behind; delete the file and stop the stale `varta dev` by hand.
# 3. Each group (src/tests/sdk/main.ts) is one execution with production budgets and
#    must end with `SUMMARY N passed, 0 failed`.
# Prints PASS or FAIL per scenario and exits nonzero on any failure.
set -eu
# A relative VARTA_BIN names a path from the caller's directory.
varta="${VARTA_BIN:-varta}"
case "$varta" in
  /*) ;;
  */*) varta="$(pwd)/$varta" ;;
esac
cd "$(dirname "$0")/.."

placeholder="http://127.0.0.1:1"
listening_prefix="varta listening: "
startup_seconds=120
groups="${*:-sdk-unary sdk-problem sdk-retry sdk-abort sdk-timeout sdk-sse}"

if [ ! -d node_modules/uarp-sdk/src ]; then
  echo "missing node_modules/uarp-sdk/src: run npm ci --ignore-scripts" >&2
  exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/varta-sdk-loopback.XXXXXX")
# Matches the ignore rule `varta.sdk-test.loopback-*.json` in .gitignore.
config="varta.sdk-test.loopback-$$.json"
server=""
cleanup() {
  if [ -n "$server" ]; then
    kill "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
  fi
  rm -f "$config"
  rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

"$varta" dev --config varta.sdk-server.json > "$work/server.out" 2> "$work/server.err" &
server=$!

origin=""
waited=0
while [ -z "$origin" ]; do
  origin=$(sed -n "s|^$listening_prefix\(http://127\.0\.0\.1:[0-9][0-9]*\)$|\1|p" "$work/server.err" | head -n 1)
  [ -n "$origin" ] && break
  if ! kill -0 "$server" 2>/dev/null || [ "$waited" -ge "$startup_seconds" ]; then
    echo "FAIL server: no listening address" >&2
    cat "$work/server.err" >&2
    exit 1
  fi
  sleep 1
  waited=$((waited + 1))
done
echo "mock API on $origin"
sed "s|\"$placeholder/\*\*\"|\"$origin/**\"|" varta.sdk-test.json > "$config"
if ! grep -q "\"$origin/\*\*\"" "$config"; then
  echo "FAIL config: placeholder $placeholder not found in varta.sdk-test.json" >&2
  exit 1
fi

status=0
for group in $groups; do
  output=$(TEST_GROUP="$group" SDK_BASE_URL="$origin" "$varta" dev --config "$config" 2>&1) || true
  printf '%s\n' "$output" | sed -n 's/^varta log: //p' | grep -E '^(FAIL|  expected|  actual|UNKNOWN)' | sed "s/^/[$group] /" || true
  summary=$(printf '%s\n' "$output" | sed -n 's/^varta log: \(SUMMARY .*\)$/\1/p')
  if printf '%s\n' "$summary" | grep -q "^SUMMARY [1-9][0-9]* passed, 0 failed$"; then
    printf 'PASS %s: %s\n' "$group" "$summary"
  else
    printf 'FAIL %s: %s\n' "$group" "${summary:-$(printf '%s\n' "$output" | grep -m1 -E 'varta failed|fuel|deadline' || echo 'no summary')}"
    status=1
  fi
done
if ! kill -0 "$server" 2>/dev/null; then
  echo "FAIL server: exited during the run" >&2
  cat "$work/server.err" >&2
  status=1
fi
exit $status
