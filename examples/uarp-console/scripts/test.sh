#!/bin/sh
# Runs the console's Varta-executed tests group by group, then the view tests:
#   VARTA_BIN=/path/to/varta scripts/test.sh [group…]
# 1. The `app-*` groups need the product build and the loopback mock: the run
#    builds varta.json (`app-manifest` reads its manifest) and starts the mock
#    (scripts/mock.sh) for app-start, app-e2e, app-e2e-long, app-stitch and
#    app-settings. `app-reconnects` is a measurement, run only when named.
# 2. varta.test.json grants `net.request` on a placeholder origin; the run writes
#    a copy with the mock's bound origin (an exact grant: ports are never
#    wildcards) as `varta.test.loopback-<pid>.json` (git-ignored, removed on exit)
#    and builds the test entry once. A run killed without its exit trap leaves
#    that file and possibly the mock behind; delete and stop them by hand.
# 3. Each group is one `varta run` of that build (TEST_GROUP=<name>) under a host
#    policy derived from the built manifest (dist/tests.manifest.json): its ABI
#    version and grants as they are, its virtual roots mapped (`project` to this
#    directory, `data`, the KV databases, to a fresh temporary directory). Unit
#    groups are bounded runs with production budgets; the app groups run as
#    application instances (--app) under the budgets of src/tests/app-ceiling.json,
#    which lower the per-turn deadline so a stream must outlive single turns
#    (app.test.ts reads the same file). Every group must end with
#    `SUMMARY N passed, 0 failed`.
# 4. Without arguments, each src/tests/view/<name>.script.json is performed
#    headlessly (TEST_VIEW=<name>) by `varta dev --view-script` and its output must
#    equal <name>.expected.json.
# 5. The API key never leaves the settings store: the test key (API_KEY in
#    src/tests/sdk/contract.ts) must not appear in the output (logs included) of
#    app-settings, and the key a view script types into #api-key must not appear
#    in that view's output (snapshot and logs). No API key is built into the
#    product: no string of the key form (`uarp_<prefix>_<secret>`, the test key
#    included) may appear in dist/app.wasm or dist/app.manifest.json. The
#    module stores string literals as UTF-16 code units, so the search drops
#    NUL bytes first; a plain search of the module would never find one.
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
# The host ceiling budgets of the app groups, one JSON object.
app_ceiling="src/tests/app-ceiling.json"
unit_groups="markdown-inline markdown-blocks markdown-code markdown-stream-early markdown-stream-late reactive list-mapping list-update list-move list-remove chat-turns chat-waits chat-strict chat-stream chat-dispatch chat-history chat-history-merge chat-store chat-requests chat-stitch ui-notices scenario-plain scenario-reasoning scenario-tool_call scenario-delegation scenario-approval_approve scenario-approval_reject scenario-ask_user scenario-error scenario-session_end"
app_groups="app-manifest app-start app-e2e app-e2e-long app-stitch app-settings"
groups="${*:-$unit_groups $app_groups}"
# The test key the mock accepts (step 5).
test_key=$(sed -n 's/^export const API_KEY = "\([^"]*\)";$/\1/p' src/tests/sdk/contract.ts)
if [ -z "$test_key" ]; then
  echo "FAIL config: no API_KEY in src/tests/sdk/contract.ts" >&2
  exit 1
fi
# The API key form (uarp-sdk: `uarp_<prefix>_<secret>`) searched for in the product build (step 5).
key_form='uarp_[A-Za-z0-9]+_[A-Za-z0-9_-]+'
if ! printf '%s\n' "$test_key" | grep -qE "^$key_form\$"; then
  echo "FAIL config: the test key does not have the API key form" >&2
  exit 1
fi

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

# The host policy, derived from the built manifest: its schema and ABI versions
# and its grants as they are, the package members (entry, target, policy) left
# out, each root name mapped to its directory and, when given, the budgets
# ($1, one JSON object). The manifest is varta's pretty-printed JSON, one
# top-level member per line; a root without a directory or a manifest without
# fsRoots fails the run.
mkdir "$work/data"
policy() {
  POLICY_BUDGETS="$1" PROJECT_DIR="$(pwd)" DATA_DIR="$work/data" awk '
    function fail(message) {
      print "FAIL policy: " message > "/dev/stderr"
      failed = 1
      exit 1
    }
    /^  "(entry|target|policy)": / { next }
    /^  "abiVersion": / {
      print
      if (ENVIRON["POLICY_BUDGETS"] != "") print "  \"budgets\": " ENVIRON["POLICY_BUDGETS"] ","
      next
    }
    /^  "fsRoots": \[$/ { in_roots = 1; mapped = ""; next }
    in_roots && /^  \]/ {
      in_roots = 0
      has_roots = 1
      sub(/^  \]/, "")
      print "  \"fsRoots\": {" mapped " }" $0
      next
    }
    in_roots {
      name = $0
      gsub(/[ ",]/, "", name)
      if (name == "project") dir = ENVIRON["PROJECT_DIR"]
      else if (name == "data") dir = ENVIRON["DATA_DIR"]
      else fail("no directory for the root " name)
      mapped = mapped (mapped == "" ? " " : ", ") "\"" name "\": \"" dir "\""
      next
    }
    { print }
    END { if (!failed && !has_roots) fail("no fsRoots in the manifest") }
  ' dist/tests.manifest.json
}
policy "" > "$work/unit-policy.json"
policy "$(cat "$app_ceiling")" > "$work/app-policy.json"

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
  if [ "$group" = app-settings ] && printf '%s\n' "$output" | grep -qF "$test_key"; then
    printf '[%s] FAIL the test key appears in the output\n' "$group"
    status=1
  fi
  if [ "$group" = app-manifest ]; then
    for built in dist/app.wasm dist/app.manifest.json; do
      if tr -d '\000' < "$built" | LC_ALL=C grep -aqE "$key_form"; then
        printf '[%s] FAIL a string of the API key form appears in %s\n' "$group" "$built"
        status=1
      fi
    done
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
    TEST_VIEW="$name" "$varta" dev --config varta.view.json --view-script "$script" > "$work/view.out" 2> "$work/view.err" || true
    if cmp -s "$work/view.out" "$expected"; then
      printf '[view %s] matches %s\n' "$name" "$expected"
    else
      printf '[view %s] differs from %s\n' "$name" "$expected"
      status=1
    fi
    typed_key=$(sed -n 's/.*"target":"#api-key","text":"\([^"]*\)".*/\1/p' "$script")
    if [ -n "$typed_key" ] && grep -qF "$typed_key" "$work/view.out" "$work/view.err"; then
      printf '[view %s] FAIL the typed key appears in the snapshot or the log\n' "$name"
      status=1
    fi
  done
fi
exit $status
