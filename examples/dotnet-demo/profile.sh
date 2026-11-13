#!/usr/bin/env bash
# Captures one speedscope profile per workload of dotnet-demo with dotnet-trace,
# then imports them into prism.
#
#   ./profile.sh                  # all modes, 10 s each
#   MODES="cpu lock" SECONDS_PER_MODE=5 ./profile.sh
#
# Requires: .NET SDK 8+, dotnet-trace (dotnet tool install --global dotnet-trace),
# and a built prism (cargo build -p prism-cli) or prism on PATH.
set -euo pipefail
cd "$(dirname "$0")"

MODES="${MODES:-cpu alloc lock io}"
SECONDS_PER_MODE="${SECONDS_PER_MODE:-10}"
OUT="${OUT:-profiles}"

command -v dotnet-trace >/dev/null || { echo "dotnet-trace not found. Install it with: dotnet tool install --global dotnet-trace" >&2; exit 1; }

PRISM="$(command -v prism || true)"
if [ -z "$PRISM" ]; then
  for c in ../../target/debug/prism ../../target/debug/prism.exe; do [ -x "$c" ] && PRISM="$c" && break; done
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

if [ -n "$PRISM" ]; then
  for f in "${files[@]}"; do "$PRISM" import "$f" --kind wall --tag dotnet-demo; done
  echo; echo "Open the UI with:  $PRISM ui"
else
  echo; echo "prism not found; import manually:"
  for f in "${files[@]}"; do echo "  prism import \"$f\" --kind wall"; done
fi
