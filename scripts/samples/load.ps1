<#
.SYNOPSIS
  Drive a running sample app with HTTP load (uses examples/external/loadgen.exe).
  Start the app first (start.ps1), then run this while `perfhawk record --pid <pid>` records.
.EXAMPLE
  ./scripts/samples/load.ps1 -App pocketbase -Duration 60s -Concurrency 32
  ./scripts/samples/load.ps1 -App jellyfin
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('pocketbase', 'jellyfin')][string]$App,
    [string]$Duration = '30s',
    [int]$Concurrency = 16,
    [int]$SeedRecords = 2000  # PocketBase only
)
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$loadgen = Join-Path $root 'examples\external\loadgen.exe'
if (-not (Test-Path $loadgen)) { throw 'loadgen not built. Run ./scripts/samples/setup.ps1 first.' }

function Wait-Up([string]$Url) {
    for ($i = 0; $i -lt 60; $i++) {
        try { Invoke-RestMethod $Url -TimeoutSec 2 | Out-Null; return } catch { Start-Sleep -Milliseconds 500 }
    }
    throw "$Url did not come up. Is the app running (start.ps1)?"
}

switch ($App) {
    'pocketbase' {
        $base = 'http://127.0.0.1:8090'
        Wait-Up "$base/api/health"

        $auth = Invoke-RestMethod "$base/api/collections/_superusers/auth-with-password" -Method Post `
            -ContentType 'application/json' `
            -Body (@{ identity = 'loadtest@perfhawk.test'; password = 'perfhawk-local-only' } | ConvertTo-Json)
        $h = @{ Authorization = $auth.token }

        $exists = $true
        try { Invoke-RestMethod "$base/api/collections/posts" -Headers $h | Out-Null } catch { $exists = $false }
        if (-not $exists) {
            Write-Host '==> creating "posts" collection'
            $schema = @{
                name = 'posts'; type = 'base'
                fields = @(
                    @{ name = 'title'; type = 'text'; required = $true },
                    @{ name = 'body'; type = 'text' },
                    @{ name = 'views'; type = 'number' }
                )
                listRule = ''; viewRule = ''; createRule = ''   # public: the load needs no token
            }
            Invoke-RestMethod "$base/api/collections" -Method Post -Headers $h `
                -ContentType 'application/json' -Body ($schema | ConvertTo-Json -Depth 6) | Out-Null
        }

        $have = (Invoke-RestMethod "$base/api/collections/posts/records?perPage=1" -Headers $h).totalItems
        if ($have -lt $SeedRecords) {
            Write-Host "==> seeding $($SeedRecords - $have) records"
            for ($i = $have; $i -lt $SeedRecords; $i++) {
                $rec = @{ title = "Post $i"; body = ('lorem ipsum ' * (5 + $i % 40)); views = $i % 1000 }
                Invoke-RestMethod "$base/api/collections/posts/records" -Method Post `
                    -ContentType 'application/json' -Body ($rec | ConvertTo-Json) | Out-Null
            }
        }

        # Mixed reads: health, paged lists, filtered and sorted lists.
        $urls = @(
            "$base/api/health",
            "$base/api/collections/posts/records?page=1&perPage=50&sort=-views",
            "$base/api/collections/posts/records?page=3&perPage=25&filter=(views>500)",
            "$base/api/collections/posts/records?perPage=100&filter=(title~%27Post%201%27)&sort=title"
        ) -join ','
        Write-Host "==> read load: $Concurrency workers for $Duration"
        & $loadgen -url $urls -c $Concurrency -d $Duration

        Write-Host '==> write load (creates records)'
        # Body goes via a file: Windows PowerShell 5.1 strips quotes from native-command args.
        $bodyFile = Join-Path ([IO.Path]::GetTempPath()) 'perfhawk-pb-body.json'
        Set-Content -Path $bodyFile -Value '{"title":"load","body":"lorem ipsum","views":1}' -NoNewline
        & $loadgen -url "$base/api/collections/posts/records" -method POST `
            -header 'Content-Type: application/json' -body-file $bodyFile `
            -c ([Math]::Max(1, [int]($Concurrency / 4))) -d $Duration
    }
    'jellyfin' {
        $base = 'http://127.0.0.1:8096'
        Wait-Up "$base/System/Info/Public"
        # Unauthenticated endpoints: the real ASP.NET Core pipeline (routing, middleware,
        # JSON serialization, DB-backed lookups) without needing a media library.
        $urls = @(
            "$base/System/Info/Public",
            "$base/System/Ping",
            "$base/health",
            "$base/Users/Public",
            "$base/Branding/Configuration",
            "$base/Startup/Configuration"
        ) -join ','
        Write-Host "==> load: $Concurrency workers for $Duration"
        & $loadgen -url $urls -c $Concurrency -d $Duration
    }
}
