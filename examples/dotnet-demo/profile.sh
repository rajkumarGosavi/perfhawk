#!/usr/bin/env bash
# Captures one speedscope profile per workload of dotnet-demo with dotnet-trace,
# then imports them into perfhawk.
#
#   ./profile.sh                  # all modes, 10 s each
#   MODES="cpu lock" SECONDS_PER_MODE=5 ./profile.sh
#
# Requires: .NET SDK 8+, dotnet-trace (dotnet tool install --global dotnet-trace),
# and a built perfhawk (cargo build -p perfhawk-cli) or perfhawk on PATH.
set -euo pipefail
cd "$(dirname "$0")"

MODES="${MODES:-cpu alloc lock io}"
SECONDS_PER_MODE="${SECONDS_PER_MODE:-10}"
OUT="${OUT:-profiles}"

command -v dotnet-trace >/dev/null || { echo "dotnet-trace not found. Install it with: dotnet tool install --global dotnet-trace" >&2; exit 1; }

PERFHAWK="$(command -v perfhawk || true)"
if [ -z "$PERFHAWK" ]; then
  for c in ../../target/release/perfhawk ../../target/release/perfhawk.exe ../../target/debug/perfhawk ../../target/debug/perfhawk.exe; do [ -x "$c" ] && PERFHAWK="$c" && break; done
fi

dotnet build -c Release -nologo -v q
mkdir -p "$OUT"
DLL="bin/Release/net8.0/dotnet-demo.dll"

files=()
for mode in $MODES; do
  echo "== capturing '$mode' for ${SECONDS_PER_MODE}s"
  dotnet-trace collect --format Speedscope -o "$OUT/dotnet-$mode.nettrace" -- dotnet "$DLL" "$mode" "$SECONDS_PER_MODE"
  files+=("$OUT/dotnet-$mode.speedscope.json")
done

if [ -n "$PERFHAWK" ]; then
  for f in "${files[@]}"; do "$PERFHAWK" import "$f" --kind wall --tag dotnet-demo; done
  echo; echo "Open the UI with:  $PERFHAWK ui"
else
  echo; echo "perfhawk not found; import manually:"
  for f in "${files[@]}"; do echo "  perfhawk import \"$f\" --kind wall"; done
fi
