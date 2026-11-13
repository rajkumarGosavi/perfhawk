<#
.SYNOPSIS
  Install (or upgrade, or remove) Prism on Windows. No administrator rights needed.

.EXAMPLE
  irm https://github.com/rajkumarGosavi/prism/releases/latest/download/install.ps1 | iex

.EXAMPLE
  # Pin a version, or remove Prism again:
  & ([scriptblock]::Create((irm https://github.com/rajkumarGosavi/prism/releases/latest/download/install.ps1))) -Version v0.1.0
  & ([scriptblock]::Create((irm https://github.com/rajkumarGosavi/prism/releases/latest/download/install.ps1))) -Uninstall

.DESCRIPTION
  Downloads the release zip, checks its SHA-256 against the published SHA256SUMS,
  unpacks it to %LOCALAPPDATA%\Programs\prism and adds that folder to your user PATH.
  Your profiles are never touched.
#>
[CmdletBinding()]
param(
  [string]$Version = 'latest',
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\prism'),
  [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$Repo = 'rajkumarGosavi/prism'

function Update-UserPath([string]$dir, [bool]$add) {
  $cur = [Environment]::GetEnvironmentVariable('Path', 'User')
  $parts = @($cur -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ne $dir.TrimEnd('\')) })
  if ($add) { $parts += $dir }
  [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
}

function Get-RunningPrism {
  Get-Process prism, prism-dotnet -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($InstallDir, [StringComparison]::OrdinalIgnoreCase) }
}

if ($Uninstall) {
  Get-RunningPrism | Stop-Process -Force
  if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
  Update-UserPath $InstallDir $false
  Write-Host "Prism removed from $InstallDir."
  Write-Host 'Your profiles were left in place. Delete %LOCALAPPDATA%\prism (and %APPDATA%\prism) to remove them too.'
  return
}

$api = if ($Version -eq 'latest') { "https://api.github.com/repos/$Repo/releases/latest" }
       else { "https://api.github.com/repos/$Repo/releases/tags/$Version" }
try { $rel = Invoke-RestMethod $api -Headers @{ 'User-Agent' = 'prism-installer' } }
catch { throw "Could not find release '$Version' ($($_.Exception.Message)). See https://github.com/$Repo/releases" }

$zipAsset = $rel.assets | Where-Object { $_.name -like 'prism-*-windows-x64.zip' } | Select-Object -First 1
$sumAsset = $rel.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
if (-not $zipAsset -or -not $sumAsset) { throw "Release $($rel.tag_name) has no Windows package." }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("prism-install-" + [guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null
try {
  Write-Host "Downloading Prism $($rel.tag_name) ($([math]::Round($zipAsset.size/1MB,1)) MB)..."
  $zip = Join-Path $tmp $zipAsset.name
  Invoke-WebRequest $zipAsset.browser_download_url -OutFile $zip -UserAgent 'prism-installer'
  $sums = (Invoke-WebRequest $sumAsset.browser_download_url -UserAgent 'prism-installer').Content
  $line = ($sums -split "`n" | Where-Object { $_ -match [regex]::Escape($zipAsset.name) } | Select-Object -First 1)
  if (-not $line) { throw 'SHA256SUMS does not list the package.' }
  $want = ($line -split '\s+')[0].ToLower()
  $got  = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
  if ($want -ne $got) { throw "Checksum mismatch (expected $want, got $got). Nothing was installed." }
  Write-Host 'Checksum OK.'

  if (Get-RunningPrism) { throw 'Prism is running. Close it (Ctrl+C in its window) and run the installer again.' }

  New-Item -ItemType Directory -Force $InstallDir | Out-Null
  Expand-Archive $zip $InstallDir -Force
  Update-UserPath $InstallDir $true
  $env:Path = "$env:Path;$InstallDir"

  Write-Host ''
  & (Join-Path $InstallDir 'prism.exe') --version
  Write-Host "Installed to $InstallDir"
  Write-Host 'Open a NEW terminal so the PATH change takes effect, then try:  prism --help'
}
finally { Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue }
