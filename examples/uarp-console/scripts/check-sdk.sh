#!/bin/sh
# Checks every source file of the pinned uarp-sdk, one file at a time, with
# `varta check` under the console's language profile, then instantiates its
# generic functions and methods:
#   VARTA_BIN=/path/to/varta scripts/check-sdk.sh
# The SDK sources (node_modules/uarp-sdk/src, installed by `npm ci --ignore-scripts`)
# are copied unmodified into a temporary project.
# 1. Per file: a probe entry re-exports the file with `export *` (and default-imports
#    it when it has a default export), so `varta check` compiles exactly that file and
#    what it imports. Prints one line per file and `N/N passed`.
# 2. Generics: `varta check` compiles the body of a generic function or method only
#    where it is instantiated, so step 1 accepts an uninstantiated generic body that
#    holds a refused construct. scripts/check-sdk-generics.ts instantiates each
#    generic of the SDK with concrete SDK types and is checked as the entry.
#    - Inventory: the SDK's generic declaration sites, found lexically, must equal
#      `generic_sites` below; a new or moved generic fails until the probe and the
#      list are updated.
#    - Control: for each body in `generic_bodies`, a separate copy gets a refused
#      `eval` call as the body's first statement, and the probe must be refused at
#      exactly that line. This shows the probe compiles every listed body.
#    Caveat: the inventory is a lexical scan of declaration lines, not a parse; a
#    generic written in a form it does not recognise (for example with a line break
#    before its type parameters) is neither listed nor probed. Each generic is
#    instantiated with one concrete type argument (and its declared default), so a
#    construct refused only for some other type argument is not covered.
# Exits nonzero on any failure.
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
generics_probe="scripts/check-sdk-generics.ts"
# Declaration lines of every generic function, method or class in the pinned SDK
# (`file:line`, sorted). core/sse.ts:16 is the body-less `UarpEvent.json` signature.
generic_sites="core/pagination.ts:27
core/pagination.ts:61
core/sse.ts:16
core/sse.ts:368
core/transport.ts:130
core/util.ts:8
index.ts:40"
# The line that opens each generic body: autoPaginate, collect, makeEvent's json,
# Transport.request, pick, UarpClient.request.
generic_bodies="core/pagination.ts:32 core/pagination.ts:61 core/sse.ts:368 core/transport.ts:130 core/util.ts:11 index.ts:40"
# A construct Varta never admits (it has no JavaScript engine), injected by the control.
control_statement='eval("0");'

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
config=$(printf '{"entry":"entry.ts","target":"wasm","policy":"strict","out":"dist","languageProfile":"%s"}' "$profile")
mkdir "$work/main" "$work/control"
cp -R "$sdk/src" "$work/main/sdk"
cp -R "$sdk/src" "$work/control/sdk"
printf '%s\n' "$config" > "$work/main/varta.config.json"
printf '%s\n' "$config" > "$work/control/varta.config.json"
cp "$generics_probe" "$work/control/entry.ts"
printf '%s\n' "$control_statement" > "$work/control-statement.ts"
pristine="$work/main/sdk"

files=$(cd "$pristine" && find . -name '*.ts' | sed 's|^\./||' | LC_ALL=C sort)
total=0
passed=0
for file in $files; do
  total=$((total + 1))
  module="./sdk/${file%.ts}.js"
  {
    printf 'export * from "%s";\n' "$module"
    if grep -q '^export default ' "$pristine/$file"; then
      printf 'import probeDefault from "%s";\nexport { probeDefault };\n' "$module"
    fi
    printf 'export function main(): void {}\n'
  } > "$work/main/entry.ts"
  if output=$(cd "$work/main" && "$varta" check 2>&1); then
    passed=$((passed + 1))
    printf 'ok   %s\n' "$file"
  else
    printf 'FAIL %s\n' "$file"
    printf '%s\n' "$output" | sed 's/^/  /'
  fi
done
printf '%s/%s passed\n' "$passed" "$total"

status=0
[ "$passed" -eq "$total" ] || status=1

# Generic declaration inventory. Comment lines are dropped before the scan.
found=$(cd "$pristine" && grep -rnE --include='*.ts' \
  -e '^[[:space:]]*(export[[:space:]]+)?(default[[:space:]]+)?(async[[:space:]]+)?(function[[:space:]]*\*?[[:space:]]*)?((static|readonly|public|private|protected|override|abstract)[[:space:]]+)*\*?[[:space:]]*#?[A-Za-z_$][A-Za-z0-9_$]*[[:space:]]*<' \
  -e 'class[[:space:]]+[A-Za-z_$][A-Za-z0-9_$]*[[:space:]]*<' \
  -e 'function[[:space:]]*\*?[[:space:]]*<' \
  -e '(=|\(|,)[[:space:]]*(async[[:space:]]+)?<[A-Za-z_$]' . \
  | grep -vE '^[^:]*:[0-9]+:[[:space:]]*(\*|//|/\*)' \
  | sed 's|^\./||; s|^\([^:]*:[0-9]*\):.*|\1|' | LC_ALL=C sort -u) || found=""
site_count=$(printf '%s\n' "$generic_sites" | wc -l | tr -d ' ')
if [ "$found" = "$generic_sites" ]; then
  printf 'ok   generic inventory: %s declaration sites\n' "$site_count"
else
  printf 'FAIL generic inventory: the SDK declarations differ from generic_sites; update %s and the list\n' "$generics_probe"
  printf 'expected:\n%s\nfound:\n%s\n' "$generic_sites" "$found" | sed 's/^/  /'
  status=1
fi

# Instantiation probe over the pristine copy.
cp "$generics_probe" "$work/main/entry.ts"
if output=$(cd "$work/main" && "$varta" check 2>&1); then
  printf 'ok   generic instantiation probe\n'
else
  printf 'FAIL generic instantiation probe\n'
  printf '%s\n' "$output" | sed 's/^/  /'
  status=1
fi

# Control: the probe must be refused at a statement injected into each listed body.
bodies=0
reached=0
for body in $generic_bodies; do
  bodies=$((bodies + 1))
  file=${body%:*}
  line=${body#*:}
  injected=$((line + 1))
  sed "${line}r $work/control-statement.ts" "$pristine/$file" > "$work/control/sdk/$file"
  if output=$(cd "$work/control" && "$varta" check 2>&1); then
    printf 'FAIL generic control %s: a refused statement in this body was accepted\n' "$body"
    status=1
  elif printf '%s\n' "$output" | grep -qF "/sdk/$file:$injected:"; then
    reached=$((reached + 1))
    printf 'ok   generic control %s\n' "$body"
  else
    printf 'FAIL generic control %s: refused elsewhere than line %s\n' "$body" "$injected"
    printf '%s\n' "$output" | sed 's/^/  /'
    status=1
  fi
  cp "$pristine/$file" "$work/control/sdk/$file"
done
printf 'generic bodies reached: %s/%s\n' "$reached" "$bodies"
exit $status
