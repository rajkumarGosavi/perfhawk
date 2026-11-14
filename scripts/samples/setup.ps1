<#
.SYNOPSIS
  Clone (pinned) and build the real-world sample apps: PocketBase (Go) and Jellyfin (.NET).
.EXAMPLE
  ./scripts/samples/setup.ps1
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$ext = Join-Path $root 'examples\external'
New-Item -ItemType Directory -Force $ext | Out-Null

$apps = @(
    @{ Name = 'pocketbase'; Url = 'https://github.com/pocketbase/pocketbase'; Sha = '7f2773ec234b62d0b80ca47c508397cb3d0da3ea' },
    @{ Name = 'jellyfin';   Url = 'https://github.com/jellyfin/jellyfin';     Sha = '96003befcfa451e466767a2cfb01b6f3a7a78367' }
)

function Invoke-Native([string]$What, [scriptblock]$Cmd) {
    & $Cmd
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit $LASTEXITCODE)" }
}

foreach ($app in $apps) {
    $dir = Join-Path $ext $app.Name
    if (-not (Test-Path (Join-Path $dir '.git'))) {
        Write-Host "==> cloning $($app.Name) @ $($app.Sha.Substring(0,7))"
        New-Item -ItemType Directory -Force $dir | Out-Null
        Invoke-Native 'git init' { git -C $dir init --quiet }
        Invoke-Native 'git remote add' { git -C $dir remote add origin $app.Url }
        Invoke-Native 'git fetch' { git -C $dir fetch --quiet --depth 1 origin $app.Sha }
        Invoke-Native 'git checkout' { git -C $dir checkout --quiet FETCH_HEAD }
    } else {
        Write-Host "==> $($app.Name) already cloned ($(git -C $dir rev-parse --short HEAD))"
    }
}

Write-Host '==> building PocketBase (with the perfhawk agent)'
Push-Location (Join-Path $root 'examples\pocketbase-sample')
try { Invoke-Native 'go build' { go build -o (Join-Path $ext 'pb-sample.exe') . } } finally { Pop-Location }

Write-Host '==> building Jellyfin (Release)'
Invoke-Native 'dotnet build' { dotnet build (Join-Path $ext 'jellyfin\Jellyfin.Server') -c Release --nologo -v q }

Write-Host '==> building loadgen'
Push-Location (Join-Path $root 'tools\loadgen')
try { Invoke-Native 'go build' { go build -o (Join-Path $ext 'loadgen.exe') . } } finally { Pop-Location }

Write-Host "`nDone. Next: ./scripts/samples/start.ps1 -App pocketbase|jellyfin"
