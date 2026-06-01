@echo off
REM ALVR Post-Build DLL Copy Script
REM Usage: copy-dlls.bat [--gpl]

pushd "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0copy-dlls.ps1" %*
popd
