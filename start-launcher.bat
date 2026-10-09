@echo off
cd /d "%~dp0"
if exist "realflow-launcher.exe" (
  start "" "realflow-launcher.exe"
) else if exist "target\release\realflow-launcher.exe" (
  start "" "target\release\realflow-launcher.exe"
) else (
  echo Build first: powershell -ExecutionPolicy Bypass -File scripts\build.ps1
  pause
  exit /b 1
)
