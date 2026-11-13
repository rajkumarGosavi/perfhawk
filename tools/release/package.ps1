<#
.SYNOPSIS  Assemble dist/prism-<version>-windows-x64.zip and its SHA256SUMS.
.DESCRIPTION Expects: target/release/prism.exe, target/helper/prism-dotnet.exe,
             THIRD_PARTY_NOTICES.txt (see build-notices.ps1).
#>
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
Set-Location (Resolve-Path "$PSScriptRoot/../..")

$name  = "prism-$Version-windows-x64"
$stage = "dist/stage/$name"
Remove-Item dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item target/release/prism.exe        $stage
Copy-Item target/helper/prism-dotnet.exe  $stage
Copy-Item LICENSE, THIRD_PARTY_NOTICES.txt, CHANGELOG.md $stage
Copy-Item tools/release/README.txt $stage/README.txt
Copy-Item examples $stage/examples -Recurse `
  -Exclude bin, obj, profiles

$zip = "dist/$name.zip"
Compress-Archive -Path "$stage/*" -DestinationPath $zip -CompressionLevel Optimal
Remove-Item dist/stage -Recurse -Force

$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
"$hash  $name.zip" | Set-Content -Encoding ascii dist/SHA256SUMS
Write-Host "Built $zip ($([math]::Round((Get-Item $zip).Length/1MB,1)) MB)"
Write-Host "SHA256 $hash"
