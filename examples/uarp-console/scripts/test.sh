#!/bin/sh
# Runs the console's Varta-executed tests group by group, then the view tests:
#   VARTA_BIN=/path/to/varta scripts/test.sh [group…]
# 1. The `app-*` groups need the product build and the loopback mock: the run
#    builds varta.json (`app-manifest` reads its manifest) and starts the mock
#    (scripts/mock.sh) for app-start, app-e2e and app-e2e-long.
# 2. varta.test.json grants `net.request` on a placeholder origin; the run writes
#    a copy with the mock's bound origin (an exact grant: ports are never
#    wildcards) as `varta.test.loopback-<pid>.json` (git-ignored, removed on exit)
#    and builds the test entry once. A run killed without its exit trap leaves
#    that file and possibly the mock behind; delete and stop them by hand.
# 3. Each group is one `varta run` of that build (TEST_GROUP=<name>) under a host
#    policy that repeats the build's grants exactly and maps the virtual roots:
#    `project` to this directory, `data` (the KV databases) to a fresh temporary
#    directory. Unit groups are bounded runs with production budgets; the app
#    groups run as application instances (--app) whose ceiling lowers the
#    per-turn deadline to app_deadline_ms, so a stream must outlive single turns.
#    Every group must end with `SUMMARY N passed, 0 failed`.
# 4. Without arguments, each src/tests/view/<name>.script.json is performed
#    headlessly (TEST_VIEW=<name>) by `varta dev --view-script` and its output must
#    equal <name>.expected.json.
set -eu
# A relative VARTA_BIN names a path from the caller's directory.
varta="${VARTA_BIN:-varta}"
case "$varta" in
  /*) ;;
  */*) varta="$(pwd)/$varta" ;;
esac
cd "$(dirname "$0")/.."
. scripts/mock.sh

placeholder="http://127.0.0.1:1"
# The per-turn deadline of the app groups (src/tests/app.test.ts TEST_DEADLINE_MS).
app_deadline_ms=250
unit_groups="markdown-inline markdown-blocks markdown-code markdown-stream-early markdown-stream-late reactive list-mapping list-update list-move list-remove chat-turns chat-waits chat-strict chat-stream chat-dispatch chat-history chat-history-merge chat-store chat-requests scenario-plain scenario-reasoning scenario-tool_call scenario-delegation scenario-approval_approve scenario-approval_reject scenario-ask_user scenario-error scenario-session_end"
app_groups="app-manifest app-start app-e2e app-e2e-long"
groups="${*:-$unit_groups $app_groups}"

if [ ! -d node_modules/uarp-sdk/src ]; then
  echo "missing node_modules/uarp-sdk/src: run npm ci --ignore-scripts" >&2
  exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/varta-console-test.XXXXXX")
# Matches the ignore rule `varta.test.loopback-*.json` in .gitignore.
config="varta.test.loopback-$$.json"
cleanup() {
  mock_stop
  rm -f "$config"
  rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

status=0
needs_mock=false
for group in $groups; do
  case "$group" in
    app-manifest)
      if ! "$varta" build --config varta.json > "$work/app-build.out" 2>&1; then
        printf '[app-manifest] product build failed: %s\n' "$(cat "$work/app-build.out")"
        status=1
      fi
      ;;
    app-*) needs_mock=true ;;
  esac
done

origin="$placeholder"
if [ "$needs_mock" = true ]; then
  mock_start "$varta" "$work"
  origin="$mock_origin"
fi
sed "s|\"$placeholder/\*\*\"|\"$origin/**\"|" varta.test.json > "$config"
if ! grep -q "\"$origin/\*\*\"" "$config"; then
  echo "FAIL config: placeholder $placeholder not found in varta.test.json" >&2
  exit 1
fi
if ! "$varta" build --config "$config" > "$work/build.out" 2>&1; then
  echo "FAIL build: $(cat "$work/build.out")" >&2
  exit 1
fi

# The host policy: the build's own grants (varta.test.json), the roots mapped.
mkdir "$work/data"
policy() {
  budgets="$1"
  cat <<EOF
{
  "schemaVersion": 1,
  "abiVersion": 34,$budgets
  "fsRoots": { "project": "$(pwd)", "data": "$work/data" },
  "capabilities": [
    { "id": "env.get", "patterns": ["SDK_BASE_URL", "TEST_GROUP"] },
    { "id": "fs.read", "patterns": ["/project/dist/app.manifest.json", "/project/fixtures/**"] },
    { "id": "kv.read", "patterns": ["/data/e2e-empty.kv", "/data/e2e.kv"] },
    { "id": "kv.write", "patterns": ["/data/e2e-empty.kv", "/data/e2e.kv"] },
    {
      "id": "net.request",
      "patterns": ["$origin/**"],
      "methods": ["GET", "POST"],
      "responseHeaders": ["retry-after", "x-correlation-id", "x-request-id", "x-should-retry"]
    },
    { "id": "time.now", "enabled": true },
    { "id": "time.timer", "enabled": true }
  ]
}
EOF
}
policy "" > "$work/unit-policy.json"
policy "
  \"budgets\": { \"deadlinePerTurnMs\": $app_deadline_ms }," > "$work/app-policy.json"

for group in $groups; do
  case "$group" in
    app-*) output=$(TEST_GROUP="$group" SDK_BASE_URL="$origin" "$varta" run --app --policy "$work/app-policy.json" dist/tests.wasm 2>&1) || true ;;
    *) output=$(TEST_GROUP="$group" "$varta" run --policy "$work/unit-policy.json" dist/tests.wasm 2>&1) || true ;;
  esac
  printf '%s\n' "$output" | sed -n 's/^varta log: //p' | grep -E '^(FAIL|  expected|  actual|SUMMARY|UNKNOWN)' | sed "s/^/[$group] /" || true
  if ! printf '%s\n' "$output" | grep -q "SUMMARY [1-9][0-9]* passed, 0 failed"; then
    printf '[%s] did not pass: %s\n' "$group" "$(printf '%s\n' "$output" | grep -m1 -E 'varta failed|fuel|deadline' || echo 'see output')"
    status=1
  fi
done
if [ "$needs_mock" = true ] && ! mock_alive; then
  echo "FAIL server: exited during the run" >&2
  cat "$mock_log" >&2
  status=1
fi

if [ $# -eq 0 ]; then
  for script in src/tests/view/*.script.json; do
    name=$(basename "$script" .script.json)
    expected="src/tests/view/$name.expected.json"
    if TEST_VIEW="$name" "$varta" dev --config varta.view.json --view-script "$script" 2>/dev/null | cmp -s - "$expected"; then
      printf '[view %s] matches %s\n' "$name" "$expected"
    else
      printf '[view %s] differs from %s\n' "$name" "$expected"
      status=1
    fi
  done
fi
exit $status
