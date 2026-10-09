@echo off
cd /d "%~dp0"
if exist "realflow.exe" (
  "realflow.exe" demo --steps 600 --output "%TEMP%\realflow-demo-results.json"
) else (
  "target\release\realflow.exe" demo --steps 600 --output "%TEMP%\realflow-demo-results.json"
)
set "realflow_exit=%ERRORLEVEL%"
pause
exit /b %realflow_exit%
