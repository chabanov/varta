# Starts the loopback mock of the UARP API (src/tests/sdk-server/, itself a
# Varta HTTP/1 application) for scripts/sdk-loopback.sh and scripts/test.sh,
# which source this file from the console's directory:
#   mock_start <varta> <work dir>   sets mock_pid and mock_origin, or exits 1
#   mock_alive                      succeeds while the mock still runs
#   mock_stop                       stops it (safe to call twice)
# The mock binds 127.0.0.1 on an OS-assigned port (exact grant 127.0.0.1:0) and
# reports `varta listening: <url>` on stderr; net.request ports are never
# wildcards, so callers write a config copy naming that exact origin.

mock_listening_prefix="varta listening: "
mock_startup_seconds=120
mock_pid=""
mock_origin=""
mock_log=""

mock_start() {
  mock_log="$2/server.err"
  "$1" dev --config varta.sdk-server.json > "$2/server.out" 2> "$mock_log" &
  mock_pid=$!
  waited=0
  while [ -z "$mock_origin" ]; do
    mock_origin=$(sed -n "s|^$mock_listening_prefix\(http://127\.0\.0\.1:[0-9][0-9]*\)$|\1|p" "$mock_log" | head -n 1)
    [ -n "$mock_origin" ] && break
    if ! kill -0 "$mock_pid" 2>/dev/null || [ "$waited" -ge "$mock_startup_seconds" ]; then
      echo "FAIL server: no listening address" >&2
      cat "$mock_log" >&2
      exit 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  echo "mock API on $mock_origin"
}

mock_alive() {
  [ -n "$mock_pid" ] && kill -0 "$mock_pid" 2>/dev/null
}

mock_stop() {
  if [ -n "$mock_pid" ]; then
    kill "$mock_pid" 2>/dev/null || true
    wait "$mock_pid" 2>/dev/null || true
    mock_pid=""
  fi
}
