<#
.SYNOPSIS  Unzip a release package and exercise it the way a user would.
#>
param([Parameter(Mandatory)][string]$Zip)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot/../.."
$work = Join-Path ([IO.Path]::GetTempPath()) ("perfhawk-smoke-" + [guid]::NewGuid())
New-Item -ItemType Directory $work | Out-Null
Expand-Archive (Resolve-Path $Zip) "$work/pkg"
$perfhawk = "$work/pkg/perfhawk.exe"
$home_ = "$work/home"

function Step($msg, [scriptblock]$body) {
  Write-Host "== $msg"
  & $body
  if ($LASTEXITCODE -ne 0) { throw "failed: $msg (exit $LASTEXITCODE)" }
}

foreach ($f in 'perfhawk.exe','perfhawk-dotnet.exe','LICENSE','THIRD_PARTY_NOTICES.txt','README.txt','CHANGELOG.md') {
  if (-not (Test-Path "$work/pkg/$f")) { throw "package is missing $f" }
}
Step 'perfhawk --version' { & $perfhawk --version }
Step 'import a profile' {
  & $perfhawk --home $home_ import "$root/crates/perfhawk-import/tests/fixtures/go-cpu.pb.gz"
}
Step 'ls' { & $perfhawk --home $home_ ls }
Step 'show' { & $perfhawk --home $home_ show last --top 3 }

Write-Host '== helper speaks its protocol'
$hello = '' | & "$work/pkg/perfhawk-dotnet.exe" 2>$null | Select-Object -First 1
if ($hello -notmatch '"type":"hello"') { throw "helper hello line missing: $hello" }

Write-Host '== ui serves with security headers'
$port = 17878
$p = Start-Process $perfhawk -ArgumentList '--home', $home_, 'ui', '--port', $port -PassThru -WindowStyle Hidden
try {
  $ok = $false
  foreach ($i in 1..30) {
    Start-Sleep -Milliseconds 500
    try {
      $r = Invoke-WebRequest "http://127.0.0.1:$port/" -UseBasicParsing
      if ($r.Headers['X-Frame-Options'] -eq 'DENY' -and $r.Content -match 'id="root"') { $ok = $true; break }
    } catch {}
  }
  if (-not $ok) { throw 'ui did not serve the app with X-Frame-Options' }
} finally { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }

Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
Write-Host 'Smoke test passed.'
