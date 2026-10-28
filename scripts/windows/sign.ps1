# Windows：主程序 + frpc sidecar + 安装包 一起签名
#
# 用法（PowerShell，管理员或普通用户都行）：
#   $env:WIN_CERT_PFX = "C:\certs\dev.pfx"
#   $env:WIN_CERT_PASSWORD = "p12密码"
#   .\scripts\windows\sign.ps1
#
# 或用证书指纹（CI 导入证书到 CurrentUser\My 后常用）：
#   $env:WIN_CERT_THUMBPRINT = "ABCDEF..."
#
# 可选参数：
#   -SkipBuild   复用已有产物
#   -NoBundle    只 `tauri build --no-bundle` 并签名 exe（不生成安装包）

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

function Write-Step($msg) { Write-Host "▸ $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "✓ $msg" -ForegroundColor Green }
function Fail($msg)       { Write-Host "✗ $msg" -ForegroundColor Red; exit 1 }

# ------------------------------------------------------------------ signtool
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
  throw "找不到 signtool.exe，请安装 Windows 10/11 SDK"
}

$signTool = Find-SignTool
Write-Ok "signtool: $signTool"

if (-not $PfxPath -and -not $Thumbprint) {
  Fail "需要 WIN_CERT_PFX + WIN_CERT_PASSWORD，或 WIN_CERT_THUMBPRINT"
}

function Sign-File([string]$Path) {
  if (-not (Test-Path $Path)) { throw "文件不存在：$Path" }
  $signArgs = @("sign", "/fd", "sha256", "/tr", $TimestampUrl, "/td", "sha256")
  if ($PfxPath) {
    $signArgs += @("/f", $PfxPath, "/p", $PfxPassword)
  } else {
    $signArgs += @("/sha1", $Thumbprint)
  }
  $signArgs += $Path
  & $signTool @signArgs
  if ($LASTEXITCODE -ne 0) { throw "签名失败：$Path" }
  Write-Ok "signed: $(Split-Path -Leaf $Path)"
}

function Verify-File([string]$Path) {
  & $signTool verify /pa /v $Path | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "签名校验失败：$Path" }
  Write-Ok "verified: $(Split-Path -Leaf $Path)"
}

# --------------------------------------------------------------------- 构建
if (-not $SkipBuild) {
  if ($NoBundle) {
    Write-Step "构建（--no-bundle）"
    pnpm tauri build --no-bundle
  } else {
    Write-Step "构建（--no-bundle，先签 exe 再打安装包）"
    pnpm tauri build --no-bundle
  }
  if ($LASTEXITCODE -ne 0) { Fail "构建失败" }
}

$release = "src-tauri\target\release"
$mainExe = Join-Path $release "frpc-desktop.exe"
$sidecar = Join-Path $release "frpc.exe"

# ------------------------------------------------- 先签 exe 与 sidecar（顺序重要）
Write-Step "签名主程序与 frpc sidecar"
Sign-File $mainExe
if (Test-Path $sidecar) {
  Sign-File $sidecar
} else {
  Write-Host "  未发现 sidecar（可能没内置 frpc），跳过" -ForegroundColor Yellow
}

if ($NoBundle) {
  Verify-File $mainExe
  Write-Ok "已跳过安装包（-NoBundle）"
  exit 0
}

# ------------------------------------------------------------------ 打安装包
Write-Step "生成安装包"
pnpm tauri bundle
if ($LASTEXITCODE -ne 0) { Fail "打包失败" }

# --------------------------------------------- 再签安装包（用户双击的就是它）
$installers = @()
foreach ($dir in @("$release\bundle\msi", "$release\bundle\nsis")) {
  if (Test-Path $dir) {
    $installers += Get-ChildItem -Path $dir -Include *.msi, *.exe -Recurse -File
  }
}
if ($installers.Count -eq 0) { Fail "没找到安装包产物" }

Write-Step "签名安装包"
foreach ($f in $installers) { Sign-File $f.FullName }

# --------------------------------------------------------------------- 自查
Write-Step "签名自查"
Verify-File $mainExe
foreach ($f in $installers) { Verify-File $f.FullName }

Write-Host ""
Write-Ok "完成。产物："
$installers | ForEach-Object { Write-Host "  $($_.FullName)" }
Write-Host ""
Write-Host "下一步提醒：" -ForegroundColor Yellow
Write-Host "  1) 首次分发前，把安装包提交 Microsoft 误报申诉（HackTool:Win32/Frp 很常见）"
Write-Host "     https://www.microsoft.com/en-us/wdsi/filesubmission"
Write-Host "  2) OV 证书需要积累下载量才会让 SmartScreen 转白；EV 证书立即生效"
