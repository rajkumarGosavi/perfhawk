<#
.SYNOPSIS
  One command: start a sample app, put it under load, record it with perfhawk, stop it.
.EXAMPLE
  ./scripts/samples/record.ps1 -App pocketbase
  ./scripts/samples/record.ps1 -App jellyfin -Seconds 30 -Kinds cpu,memory -Open
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('pocketbase', 'jellyfin')][string]$App,
    [int]$Seconds = 20,
    [string]$Kinds,   # e.g. cpu,memory ; default is perfhawk's own
    [switch]$Open,    # open the recording in the browser UI
    [switch]$Doctor   # run `perfhawk doctor` (a written diagnosis) instead of `perfhawk record`
)
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$ext = Join-Path $root 'examples\external'
$samples = $PSScriptRoot
$run = Join-Path $ext ".run\$App"

# Pick up tools installed since this shell started (e.g. ffmpeg via winget).
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')

if (-not (Test-Path (Join-Path $ext 'loadgen.exe'))) { & (Join-Path $samples 'setup.ps1') }

$pf = @('release', 'debug') | ForEach-Object { Join-Path $root "target\$_\perfhawk.exe" } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $pf) {
    Write-Host '==> building perfhawk'
    Push-Location $root
    try { cargo build -p perfhawk-cli --quiet; if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' } } finally { Pop-Location }
    $pf = Join-Path $root 'target\debug\perfhawk.exe'
}

if ($App -eq 'jellyfin' -and -not $env:PERFHAWK_DOTNET -and -not (Test-Path (Join-Path (Split-Path $pf) 'perfhawk-dotnet.exe'))) {
    # .NET recording needs the perfhawk-dotnet helper; build it once, next to the samples' state.
    $helperDir = Join-Path $ext '.run\perfhawk-dotnet'
    $helperExe = Join-Path $helperDir 'perfhawk-dotnet.exe'
    if (-not (Test-Path $helperExe)) {
        Write-Host '==> building the perfhawk-dotnet helper'
        dotnet publish (Join-Path $root 'helpers\perfhawk-dotnet') -c Release -o $helperDir --nologo -v q
        if ($LASTEXITCODE -ne 0) { throw 'dotnet publish of the helper failed' }
    }
    $env:PERFHAWK_DOTNET = $helperExe
}

$appProc = $null; $load = $null
try {
    Write-Host "==> starting $App"
    $appProc = & (Join-Path $samples 'start.ps1') -App $App -Fresh -Background
    $loadLog = Join-Path $run 'load.log'
    New-Item -ItemType Directory -Force $run | Out-Null
    Remove-Item $loadLog -ErrorAction SilentlyContinue

    # Load outlasts the recording: seeding/warm-up happens first, then $Seconds + margin of traffic.
    $load = Start-Process powershell -PassThru -WindowStyle Hidden -RedirectStandardOutput $loadLog `
        -ArgumentList '-NoProfile', '-File', (Join-Path $samples 'load.ps1'), '-App', $App, '-Duration', "$($Seconds + 15)s"

    Write-Host '==> waiting for the app to come up and the load to start'
    for ($i = 0; $i -lt 180; $i++) {
        if ($appProc.HasExited) { throw "$App exited early; see $run\app.err.log" }
        if ($load.HasExited -and -not (Select-String -Path $loadLog -Pattern 'load:' -Quiet -ErrorAction SilentlyContinue)) {
            throw "load.ps1 failed; see $loadLog"
        }
        if (Select-String -Path $loadLog -Pattern 'load:' -Quiet -ErrorAction SilentlyContinue) { break }
        Start-Sleep -Seconds 1
    }

    Write-Host "==> recording pid $($appProc.Id) for ${Seconds}s"
    $args = @($(if ($Doctor) { 'doctor' } else { 'record' }), '--pid', $appProc.Id, '--duration', "${Seconds}s")
    if ($Kinds -and -not $Doctor) { $args += @('--kinds', $Kinds) }
    if ($Open -and -not $Doctor) { $args += '--open' }
    & $pf @args
    if ($LASTEXITCODE -ne 0) { throw "perfhawk failed (exit $LASTEXITCODE)" }
}
finally {
    if ($load -and -not $load.HasExited) { Stop-Process -Id $load.Id -Force -ErrorAction SilentlyContinue }
    Get-CimInstance Win32_Process -Filter "Name='loadgen.exe'" -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    if ($appProc -and -not $appProc.HasExited) { Stop-Process -Id $appProc.Id -Force -ErrorAction SilentlyContinue }
}
