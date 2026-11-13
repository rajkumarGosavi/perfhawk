<#
.SYNOPSIS
  Install (or upgrade, or remove) Perfhawk on Windows. No administrator rights needed.

.EXAMPLE
  irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1 | iex

.EXAMPLE
  # Pin a version, or remove Perfhawk again:
  & ([scriptblock]::Create((irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1))) -Version v0.1.0
  & ([scriptblock]::Create((irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1))) -Uninstall

.DESCRIPTION
  Downloads the release zip, checks its SHA-256 against the published SHA256SUMS,
  unpacks it to %LOCALAPPDATA%\Programs\perfhawk and adds that folder to your user PATH.
  Your profiles are never touched.
#>
[CmdletBinding()]
param(
  [string]$Version = 'latest',
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\perfhawk'),
  [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$Repo = 'rajkumarGosavi/perfhawk'

function Update-UserPath([string]$dir, [bool]$add) {
  $cur = [Environment]::GetEnvironmentVariable('Path', 'User')
  $parts = @($cur -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ne $dir.TrimEnd('\')) })
  if ($add) { $parts += $dir }
  [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
}

function Get-RunningPerfhawk {
  Get-Process perfhawk, perfhawk-dotnet -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($InstallDir, [StringComparison]::OrdinalIgnoreCase) }
}

if ($Uninstall) {
  Get-RunningPerfhawk | Stop-Process -Force
  if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
  Update-UserPath $InstallDir $false
  Write-Host "Perfhawk removed from $InstallDir."
  Write-Host 'Your profiles were left in place. Delete %LOCALAPPDATA%\perfhawk (and %APPDATA%\perfhawk) to remove them too.'
  return
}

$api = if ($Version -eq 'latest') { "https://api.github.com/repos/$Repo/releases/latest" }
       else { "https://api.github.com/repos/$Repo/releases/tags/$Version" }
try { $rel = Invoke-RestMethod $api -Headers @{ 'User-Agent' = 'perfhawk-installer' } }
catch { throw "Could not find release '$Version' ($($_.Exception.Message)). See https://github.com/$Repo/releases" }

$zipAsset = $rel.assets | Where-Object { $_.name -like 'perfhawk-*-windows-x64.zip' } | Select-Object -First 1
$sumAsset = $rel.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
if (-not $zipAsset -or -not $sumAsset) { throw "Release $($rel.tag_name) has no Windows package." }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("perfhawk-install-" + [guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null
try {
  Write-Host "Downloading Perfhawk $($rel.tag_name) ($([math]::Round($zipAsset.size/1MB,1)) MB)..."
  $zip = Join-Path $tmp $zipAsset.name
  Invoke-WebRequest $zipAsset.browser_download_url -OutFile $zip -UserAgent 'perfhawk-installer'
  $sums = (Invoke-WebRequest $sumAsset.browser_download_url -UserAgent 'perfhawk-installer').Content
  $line = ($sums -split "`n" | Where-Object { $_ -match [regex]::Escape($zipAsset.name) } | Select-Object -First 1)
  if (-not $line) { throw 'SHA256SUMS does not list the package.' }
  $want = ($line -split '\s+')[0].ToLower()
  $got  = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
  if ($want -ne $got) { throw "Checksum mismatch (expected $want, got $got). Nothing was installed." }
  Write-Host 'Checksum OK.'

  if (Get-RunningPerfhawk) { throw 'Perfhawk is running. Close it (Ctrl+C in its window) and run the installer again.' }

  New-Item -ItemType Directory -Force $InstallDir | Out-Null
  Expand-Archive $zip $InstallDir -Force
  Update-UserPath $InstallDir $true
  $env:Path = "$env:Path;$InstallDir"

  Write-Host ''
  & (Join-Path $InstallDir 'perfhawk.exe') --version
  Write-Host "Installed to $InstallDir"
  Write-Host 'Open a NEW terminal so the PATH change takes effect, then try:  perfhawk --help'
}
finally { Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue }
