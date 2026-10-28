@echo off
REM Windows build helper: load MSVC env AND hand the lib dirs to rustc explicitly.
REM
REM Two problems this solves:
REM  1) rustc calls link.exe directly; without vcvars, LNK1104 "cannot open msvcrt.lib".
REM  2) even with vcvars loaded, the env did not reach the linker reliably here, so we
REM     also pass -Lnative for the MSVC / UCRT / UM lib folders via RUSTFLAGS.
REM NOTE: keep this file ASCII-only (cmd.exe reads it in the OEM codepage).
setlocal

set "VSW_HERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSW_HERE%" (
  echo [x] vswhere.exe not found. Install Visual Studio 2022+ with the C++ build tools.
  exit /b 1
)

for /f "usebackq tokens=*" %%i in (`"%VSW_HERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%i"
if not defined VSPATH (
  echo [x] No Visual Studio instance with the C++ toolset found.
  exit /b 1
)
echo [*] VS: %VSPATH%

call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 echo [x] vcvars64.bat failed & exit /b 1

REM newest MSVC toolset
set "MSVC_TOOLSET="
for /d %%d in ("%VSPATH%\VC\Tools\MSVC\*") do set "MSVC_TOOLSET=%%~nxd"
set "MSVC_LIB=%VSPATH%\VC\Tools\MSVC\%MSVC_TOOLSET%\lib\x64"

REM newest Windows 10/11 SDK
set "SDKVER="
for /f "tokens=*" %%v in ('dir /b /ad /o-n "%ProgramFiles(x86)%\Windows Kits\10\Lib" 2^>nul') do if not defined SDKVER set "SDKVER=%%v"
set "SDK_UCrt=%ProgramFiles(x86)%\Windows Kits\10\Lib\%SDKVER%\ucrt\x64"
set "SDK_UM=%ProgramFiles(x86)%\Windows Kits\10\Lib\%SDKVER%\um\x64"

echo [*] MSVC toolset: %MSVC_TOOLSET%
echo [*] SDK: %SDKVER%
if not exist "%MSVC_LIB%\libcmt.lib" echo [x] libcmt.lib missing in %MSVC_LIB% & exit /b 1
if not exist "%MSVC_LIB%\msvcrt.lib" echo [!] dynamic CRT import libs absent, using static CRT (+crt-static)

REM oldnames.lib is missing from this toolset; link.exe still wants the file.
REM scripts\make-oldnames-shim.cmd generates an empty one into src-tauri\msvc-shim.
set "SHIM=%~dp0..\src-tauri\msvc-shim"
if exist "%SHIM%\oldnames.lib" (
  set "LIB=%SHIM%;%LIB%"
  echo [*] using oldnames shim
)

REM This machine's MSVC 14.50 x64 toolset is missing the dynamic CRT import libs
REM (msvcrt.lib / ucrt.lib / vcruntime.lib), so we link the static CRT instead.
REM libcmt.lib IS present, which makes +crt-static the reliable choice here.
REM Note: do NOT add -Lnative=<path> here. This machine's paths contain spaces
REM ("Program Files"), RUSTFLAGS is whitespace-split and rustc then sees garbage
REM ("multiple input filenames provided"). vcvars' LIB already covers these dirs.
set "RUSTFLAGS=-C target-feature=+crt-static"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set CARGO_TERM_COLOR=never

cd /d "%~dp0.."

if not "%1"=="" (
  echo [*] pnpm tauri build %*
  call pnpm tauri build %*
) else (
  echo [*] pnpm tauri build --no-bundle
  call pnpm tauri build --no-bundle
)

set "RC=%errorlevel%"
echo [*] exit=%RC%
exit /b %RC%
