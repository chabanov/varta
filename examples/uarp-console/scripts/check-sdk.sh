#!/bin/sh
# Checks every source file of the pinned uarp-sdk, one file at a time, with
# `varta check` under the console's language profile:
#   VARTA_BIN=/path/to/varta scripts/check-sdk.sh
# The SDK sources (node_modules/uarp-sdk/src, installed by `npm ci --ignore-scripts`)
# are copied unmodified into a temporary project. For each file a probe entry
# re-exports it with `export *` (and default-imports it when it has a default
# export), so `varta check` compiles exactly that file and what it imports.
# Prints one line per file and `N/N passed`; exits nonzero on any failure.
set -eu
# A relative VARTA_BIN names a path from the caller's directory.
varta="${VARTA_BIN:-varta}"
case "$varta" in
  /*) ;;
  */*) varta="$(pwd)/$varta" ;;
esac
cd "$(dirname "$0")/.."
profile="checked-f64-utf16-v38"
pinned="0.13.2"
sdk="node_modules/uarp-sdk"

if [ ! -d "$sdk/src" ]; then
  echo "missing $sdk/src: run npm ci --ignore-scripts" >&2
  exit 2
fi
version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$sdk/package.json")
if [ "$version" != "$pinned" ]; then
  echo "uarp-sdk $version is installed; the probe expects the pinned $pinned" >&2
  exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/varta-sdk-check.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM
cp -R "$sdk/src" "$work/sdk"
printf '{"entry":"entry.ts","target":"wasm","policy":"strict","out":"dist","languageProfile":"%s"}\n' "$profile" > "$work/varta.config.json"

files=$(cd "$work/sdk" && find . -name '*.ts' | sed 's|^\./||' | LC_ALL=C sort)
total=0
passed=0
for file in $files; do
  total=$((total + 1))
  module="./sdk/${file%.ts}.js"
  {
    printf 'export * from "%s";\n' "$module"
    if grep -q '^export default ' "$work/sdk/$file"; then
      printf 'import probeDefault from "%s";\nexport { probeDefault };\n' "$module"
    fi
    printf 'export function main(): void {}\n'
  } > "$work/entry.ts"
  if output=$(cd "$work" && "$varta" check 2>&1); then
    passed=$((passed + 1))
    printf 'ok   %s\n' "$file"
  else
    printf 'FAIL %s\n' "$file"
    printf '%s\n' "$output" | sed 's/^/  /'
  fi
done
printf '%s/%s passed\n' "$passed" "$total"
[ "$passed" -eq "$total" ]
