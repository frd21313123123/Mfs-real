@echo off
cd /d "%~dp0"
py -3 -m realflow doctor
py -3 -m realflow demo --steps 480 --output demo-results.json
pause
