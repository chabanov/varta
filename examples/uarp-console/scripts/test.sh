#!/bin/sh
# Runs the console's Varta-executed tests group by group, then the view tests:
# each src/tests/view/<name>.script.json is performed headlessly (TEST_VIEW=<name>) by
# `varta dev --view-script` and its output must equal <name>.expected.json.
#   VARTA_BIN=/path/to/varta scripts/test.sh [group…]
set -eu
cd "$(dirname "$0")/.."
varta="${VARTA_BIN:-varta}"
groups="${*:-markdown-inline markdown-blocks markdown-code markdown-stream-early markdown-stream-late reactive list-mapping list-update list-move list-remove chat-turns chat-waits chat-strict chat-stream chat-dispatch chat-history chat-history-merge chat-store chat-requests scenario-plain scenario-reasoning scenario-tool_call scenario-delegation scenario-approval_approve scenario-approval_reject scenario-ask_user scenario-error scenario-session_end}"
status=0
for group in $groups; do
  output=$(TEST_GROUP="$group" "$varta" dev --config varta.test.json 2>&1) || true
  printf '%s\n' "$output" | sed -n 's/^varta log: //p' | grep -E '^(FAIL|  expected|  actual|SUMMARY|UNKNOWN)' | sed "s/^/[$group] /" || true
  if ! printf '%s\n' "$output" | grep -q "SUMMARY [0-9]* passed, 0 failed"; then
    printf '[%s] did not pass: %s\n' "$group" "$(printf '%s\n' "$output" | grep -m1 -E 'varta failed|fuel' || echo 'see output')"
    status=1
  fi
done
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
