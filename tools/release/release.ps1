<#
.SYNOPSIS
  One-command release: checks, tags, pushes. CI does the rest and publishes.
.EXAMPLE
  ./tools/release/release.ps1 -Version 0.1.0-rc.1   # pre-release
  ./tools/release/release.ps1 -Version 0.1.0        # stable (becomes "latest")
  ./tools/release/release.ps1 -Version 0.1.0 -DryRun
#>
param(
  [Parameter(Mandatory)][string]$Version,
  [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
Set-Location (Resolve-Path "$PSScriptRoot/../..")

$Version = $Version.TrimStart('v')
if ($Version -notmatch '^(\d+\.\d+\.\d+)(-[0-9A-Za-z.]+)?$') { throw "Bad version '$Version'. Use 0.1.0 or 0.1.0-rc.1" }
$core = $Matches[1]
$stable = -not $Matches[2]
$tag = "v$Version"

function Fail($m) { throw $m }
function Invoke-Git { & git @args; if ($LASTEXITCODE) { Fail "git $args failed" } }

if ((git rev-parse --abbrev-ref HEAD) -ne 'main') { Fail 'Not on main.' }
if (git status --porcelain) { Fail 'Working tree not clean.' }
Invoke-Git fetch origin main --tags --quiet
if ((git rev-parse HEAD) -ne (git rev-parse origin/main)) { Fail 'main is not equal to origin/main (push or pull first).' }

$cargo = ((Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value)
$csproj = ((Select-String -Path helpers/perfhawk-dotnet/PerfhawkDotnet.csproj -Pattern '<Version>(.+)</Version>' | Select-Object -First 1).Matches[0].Groups[1].Value)
if ($cargo -ne $core) { Fail "Cargo.toml version is $cargo, expected $core." }
if ($csproj -ne $core) { Fail "PerfhawkDotnet.csproj version is $csproj, expected $core." }
if ($stable -and -not (Select-String -Path CHANGELOG.md -Pattern "^## \[$([regex]::Escape($core))\] - \d{4}-\d{2}-\d{2}" -Quiet)) {
  Fail "CHANGELOG.md needs a dated '## [$core] - YYYY-MM-DD' section (not 'unreleased')."
}
if (git ls-remote --tags origin $tag) { Fail "Tag $tag already exists on origin. Never reuse a tag; bump the version." }

$goTag = "agents/go/v$core"
$doGo = $stable -and -not (git ls-remote --tags origin $goTag)

Write-Host "Release $tag ($(if ($stable) {'stable'} else {'pre-release'}))"
if ($doGo) { Write-Host "Also tagging $goTag" }
if ($DryRun) { Write-Host 'Dry run: all checks passed, nothing tagged.'; return }

Invoke-Git tag -a $tag -m $tag
Invoke-Git push origin $tag
if ($doGo) { Invoke-Git tag -a $goTag -m $goTag; Invoke-Git push origin $goTag }
Write-Host "Pushed. Watch: https://github.com/rajkumarGosavi/perfhawk/actions/workflows/release.yml"
