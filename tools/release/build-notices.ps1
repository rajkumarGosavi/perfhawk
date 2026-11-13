<#
.SYNOPSIS  Generate THIRD_PARTY_NOTICES.txt: Rust crates, npm production
           dependencies embedded in the UI bundle, and other bundled parts.
.NOTES     Needs cargo-about (cargo install cargo-about --locked --features cli)
           and `npm ci` already run in ui/.
#>
param([string]$Output = 'THIRD_PARTY_NOTICES.txt')
$ErrorActionPreference = 'Stop'
Set-Location (Resolve-Path "$PSScriptRoot/../..")

$parts = New-Object System.Collections.Generic.List[string]
$parts.Add("PRISM THIRD-PARTY NOTICES`r`nPrism itself is Apache-2.0 (see LICENSE). It includes the following third-party software.`r`n`r`n")

$tmp = Join-Path ([IO.Path]::GetTempPath()) 'prism-rust-notices.txt'
$ErrorActionPreference = 'Continue'   # cargo-about logs to stderr; Windows PowerShell 5.1 treats that as an error
cargo about generate about.hbs --workspace -o $tmp
$code = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($code -ne 0) { throw 'cargo about failed' }
$parts.Add((Get-Content $tmp -Raw))

$parts.Add("`r`n`r`nJAVASCRIPT DEPENDENCIES OF THE EMBEDDED UI`r`n=====================================`r`n")
Push-Location ui
$dirs = & npm ls --omit=dev --all --parseable
Pop-Location
foreach ($d in $dirs | Where-Object { $_ -and (Test-Path "$_/package.json") } | Sort-Object -Unique) {
  $pkg = Get-Content "$d/package.json" -Raw | ConvertFrom-Json
  if ($pkg.name -eq 'ui') { continue }
  $lic = Get-ChildItem $d -File | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING)' } | Select-Object -First 1
  $text = if ($lic) { Get-Content $lic.FullName -Raw } else { "(no licence file shipped; declared licence: $($pkg.license))" }
  $parts.Add("`r`n--------------------------------------------------------------------------------`r`n$($pkg.name) $($pkg.version) ($($pkg.license))`r`n`r`n$text")
}

$parts.Add("`r`n`r`n" + (Get-Content tools/release/NOTICES-extra.txt -Raw))
[IO.File]::WriteAllText((Join-Path (Get-Location) $Output), ($parts -join ''), (New-Object Text.UTF8Encoding $false))
Write-Host "Wrote $Output ($([math]::Round((Get-Item $Output).Length/1KB)) KB)"
