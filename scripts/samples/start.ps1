<#
.SYNOPSIS
  Launch a sample app in the foreground (state under examples/external/.run/<app>).
  Then run `perfhawk record --pid <pid>` against it (find the pid with Get-Process).
.EXAMPLE
  ./scripts/samples/start.ps1 -App pocketbase
  ./scripts/samples/start.ps1 -App jellyfin
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('pocketbase', 'jellyfin')][string]$App,
    [switch]$Fresh  # wipe the app's local data first
)
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$ext = Join-Path $root 'examples\external'
$run = Join-Path $ext ".run\$App"
if ($Fresh -and (Test-Path $run)) { Remove-Item -Recurse -Force $run }
New-Item -ItemType Directory -Force $run | Out-Null

switch ($App) {
    'pocketbase' {
        $exe = Join-Path $ext 'pb-sample.exe'
        if (-not (Test-Path $exe)) { throw 'Not built. Run ./scripts/samples/setup.ps1 first.' }
        $data = Join-Path $run 'pb_data'
        # Local-only superuser so load.ps1 can seed data. Never reuse these anywhere real.
        & $exe superuser upsert 'loadtest@perfhawk.test' 'perfhawk-local-only' --dir $data | Out-Null
        Write-Host 'PocketBase on http://127.0.0.1:8090 (Ctrl+C to stop)'
        & $exe serve --http 127.0.0.1:8090 --dir $data
    }
    'jellyfin' {
        $dll = Join-Path $ext 'jellyfin\Jellyfin.Server\bin\Release\net10.0\jellyfin.dll'
        if (-not (Test-Path $dll)) { throw 'Not built. Run ./scripts/samples/setup.ps1 first.' }
        Write-Host 'Jellyfin on http://127.0.0.1:8096 (Ctrl+C to stop)'
        # --nowebclient: the API is what we load-test; no separate web UI checkout needed.
        dotnet $dll --nowebclient `
            --datadir (Join-Path $run 'data') --configdir (Join-Path $run 'config') `
            --cachedir (Join-Path $run 'cache') --logdir (Join-Path $run 'log')
    }
}
