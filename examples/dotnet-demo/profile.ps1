# Captures one speedscope profile per workload of dotnet-demo with dotnet-trace,
# then imports them into perfhawk.
#
#   ./profile.ps1                 # all modes, 10 s each
#   ./profile.ps1 -Modes cpu,lock -Seconds 5
#
# Requires: .NET SDK 8+, dotnet-trace (dotnet tool install --global dotnet-trace),
# and a built perfhawk (cargo build -p perfhawk-cli) or perfhawk on PATH.
param(
    [string[]]$Modes = @("cpu", "alloc", "lock", "io"),
    [int]$Seconds = 10,
    [string]$Out = "profiles"
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not (Get-Command dotnet-trace -ErrorAction SilentlyContinue)) {
    Write-Error "dotnet-trace not found. Install it with:  dotnet tool install --global dotnet-trace"
}

$perfhawk = (Get-Command perfhawk -ErrorAction SilentlyContinue).Source
if (-not $perfhawk) {
    foreach ($profile in "release", "debug") {
        $candidate = Join-Path $PSScriptRoot "..\..\target\$profile\perfhawk.exe"
        if (Test-Path $candidate) { $perfhawk = (Resolve-Path $candidate).Path; break }
    }
}

dotnet build -c Release -nologo -v q
New-Item -ItemType Directory -Force $Out | Out-Null
$dll = Join-Path $PSScriptRoot "bin\Release\net8.0\dotnet-demo.dll"

$files = @()
foreach ($mode in $Modes) {
    $trace = Join-Path $Out "dotnet-$mode.nettrace"
    Write-Host "== capturing '$mode' for $Seconds s"
    dotnet-trace collect --format Speedscope -o $trace -- dotnet $dll $mode $Seconds
    $files += (Join-Path $Out "dotnet-$mode.speedscope.json")
}

if ($perfhawk) {
    foreach ($f in $files) { & $perfhawk import $f --kind wall --tag dotnet-demo }
    Write-Host "`nOpen the UI with:  $perfhawk ui"
} else {
    Write-Host "`nperfhawk not found; import manually:"
    foreach ($f in $files) { Write-Host "  perfhawk import `"$f`" --kind wall" }
}
