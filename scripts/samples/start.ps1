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
    [switch]$Fresh,      # wipe the app's local data first
    [switch]$Background  # start detached, log to .run/<app>/app.log, and output the Process
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
        $argList = @('serve', '--http', '127.0.0.1:8090', '--dir', $data)
        if ($Background) {
            return Start-Process $exe -ArgumentList $argList -PassThru -WindowStyle Hidden `
                -RedirectStandardOutput (Join-Path $run 'app.log') -RedirectStandardError (Join-Path $run 'app.err.log')
        }
        Write-Host 'PocketBase on http://127.0.0.1:8090 (Ctrl+C to stop)'
        & $exe @argList
    }
    'jellyfin' {
        $dll = Join-Path $ext 'jellyfin\Jellyfin.Server\bin\Release\net10.0\jellyfin.dll'
        if (-not (Test-Path $dll)) { throw 'Not built. Run ./scripts/samples/setup.ps1 first.' }
        # Jellyfin refuses to start without a working ffmpeg (even with no media library).
        if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
            throw 'Jellyfin needs ffmpeg on PATH. Install it (e.g. `winget install Gyan.FFmpeg`) and reopen the shell.'
        }
        # --nowebclient: the API is what we load-test; no separate web UI checkout needed.
        $argList = @($dll, '--nowebclient',
            '--datadir', (Join-Path $run 'data'), '--configdir', (Join-Path $run 'config'),
            '--cachedir', (Join-Path $run 'cache'), '--logdir', (Join-Path $run 'log'))
        if ($Background) {
            return Start-Process dotnet -ArgumentList $argList -PassThru -WindowStyle Hidden `
                -RedirectStandardOutput (Join-Path $run 'app.log') -RedirectStandardError (Join-Path $run 'app.err.log')
        }
        Write-Host 'Jellyfin on http://127.0.0.1:8096 (Ctrl+C to stop)'
        & dotnet @argList
    }
}
