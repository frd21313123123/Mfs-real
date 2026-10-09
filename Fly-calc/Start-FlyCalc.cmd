@echo off
cd /d "%~dp0"
if not exist node_modules\electron\dist\electron.exe (
  echo Install dependencies first: npm ci
  pause
  exit /b 1
)
node_modules\electron\dist\electron.exe src\desktop.cjs
