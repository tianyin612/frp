@echo off
REM Creates a shim oldnames.lib.
REM
REM This machine's MSVC 14.50 x64 toolset is missing oldnames.lib (and the dynamic
REM CRT import libs). libcmt.lib still carries a /DEFAULTLIB:oldnames directive,
REM so link.exe insists on opening the file even though nothing references it.
REM An empty import lib satisfies the linker without changing behaviour.
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul

set "SHIM=%~dp0..\src-tauri\msvc-shim"
if not exist "%SHIM%" mkdir "%SHIM%"
set "TMPD=%TEMP%\oldnames_shim"
if exist "%TMPD%" rmdir /s /q "%TMPD%"
mkdir "%TMPD%"

echo int __frpc_desktop_oldnames_shim = 0; > "%TMPD%\shim.c"
pushd "%TMPD%"
cl /nologo /c shim.c >nul
if errorlevel 1 (echo [x] cl failed & popd & exit /b 1)
lib /nologo /out:"%TMPD%\oldnames.lib" shim.obj >nul
if errorlevel 1 (echo [x] lib failed & popd & exit /b 1)
popd

copy /y "%TMPD%\oldnames.lib" "%SHIM%\oldnames.lib" >nul
if errorlevel 1 (echo [x] copy failed & exit /b 1)
echo [*] shim written: %SHIM%\oldnames.lib
dir "%SHIM%\oldnames.lib" | findstr oldnames
