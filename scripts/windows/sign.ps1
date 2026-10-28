# Windows: sign the app exe, the frpc sidecar and the installers.
#
# Usage (PowerShell):
#   $env:WIN_CERT_PFX = "C:\certs\dev.pfx"
#   $env:WIN_CERT_PASSWORD = "p12-password"
#   .\scripts\windows\sign.ps1
#
# Or with a thumbprint (cert already imported into CurrentUser\My):
#   $env:WIN_CERT_THUMBPRINT = "ABCDEF..."
#
# Flags: -SkipBuild, -NoBundle
#
# NOTE: keep this file ASCII-only. Windows PowerShell 5.1 reads .ps1 using the
# OEM/ANSI codepage; non-ASCII text corrupts parsing (string terminator errors).

[CmdletBinding()]
param(
  [string]$PfxPath = $env:WIN_CERT_PFX,
  [string]$PfxPassword = $env:WIN_CERT_PASSWORD,
  [string]$Thumbprint = $env:WIN_CERT_THUMBPRINT,
  [string]$TimestampUrl = "http://timestamp.digicert.com",
  [switch]$SkipBuild,
  [switch]$NoBundle
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

function Write-Step($msg) { Write-Host ">> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "ok $msg" -ForegroundColor Green }
function Fail($msg)       { Write-Host "xx $msg" -ForegroundColor Red; exit 1 }

function Find-SignTool {
  $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }

  $roots = @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
    "$env:ProgramFiles\Windows Kits\10\bin",
    "${env:ProgramFiles(x86)}\Windows Kits\8.1\bin"
  )
  foreach ($r in $roots) {
    if (-not (Test-Path $r)) { continue }
    $hit = Get-ChildItem -Path $r -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -match '\\x64\\' } |
      Sort-Object FullName -Descending |
      Select-Object -First 1
    if ($hit) { return $hit.FullName }
  }
  throw "signtool.exe not found. Install the Windows 10/11 SDK."
}

$signTool = Find-SignTool
Write-Ok "signtool: $signTool"

if (-not $PfxPath -and -not $Thumbprint) {
  Fail "Need WIN_CERT_PFX + WIN_CERT_PASSWORD, or WIN_CERT_THUMBPRINT"
}

function Sign-File([string]$Path) {
  if (-not (Test-Path $Path)) { throw "missing file: $Path" }
  $signArgs = @("sign", "/fd", "sha256", "/tr", $TimestampUrl, "/td", "sha256")
  if ($PfxPath) {
    $signArgs += @("/f", $PfxPath, "/p", $PfxPassword)
  } else {
    $signArgs += @("/sha1", $Thumbprint)
  }
  $signArgs += $Path
  & $signTool @signArgs
  if ($LASTEXITCODE -ne 0) { throw "sign failed: $Path" }
  Write-Ok "signed: $(Split-Path -Leaf $Path)"
}

function Verify-File([string]$Path) {
  & $signTool verify /pa /v $Path | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "verify failed: $Path" }
  Write-Ok "verified: $(Split-Path -Leaf $Path)"
}

if (-not $SkipBuild) {
  Write-Step "build (--no-bundle)"
  pnpm tauri build --no-bundle
  if ($LASTEXITCODE -ne 0) { Fail "build failed" }
}

$release = "src-tauri\target\release"
$mainExe = Join-Path $release "frpc-desktop.exe"
$sidecar = Join-Path $release "frpc.exe"

Write-Step "sign app exe and sidecar"
Sign-File $mainExe
if (Test-Path $sidecar) {
  Sign-File $sidecar
} else {
  Write-Host "  no sidecar found (frpc not bundled), skipped" -ForegroundColor Yellow
}

if ($NoBundle) {
  Verify-File $mainExe
  Write-Ok "skipped installers (-NoBundle)"
  exit 0
}

Write-Step "bundle"
pnpm tauri bundle
if ($LASTEXITCODE -ne 0) { Fail "bundle failed" }

$installers = @()
foreach ($dir in @("$release\bundle\msi", "$release\bundle\nsis")) {
  if (Test-Path $dir) {
    $installers += Get-ChildItem -Path $dir -Include *.msi, *.exe -Recurse -File
  }
}
if ($installers.Count -eq 0) { Fail "no installer produced" }

Write-Step "sign installers"
foreach ($f in $installers) { Sign-File $f.FullName }

Write-Step "verify"
Verify-File $mainExe
foreach ($f in $installers) { Verify-File $f.FullName }

Write-Host ""
Write-Ok "done:"
$installers | ForEach-Object { Write-Host "  $($_.FullName)" }
Write-Host ""
Write-Host "Reminder:" -ForegroundColor Yellow
Write-Host "  1) Submit a false-positive report to Microsoft before the first release"
Write-Host "     https://www.microsoft.com/en-us/wdsi/filesubmission"
Write-Host "  2) OV certs need download volume before SmartScreen turns green; EV is instant"
