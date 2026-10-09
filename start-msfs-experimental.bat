@echo off
cd /d "%~dp0"
echo RealFlow Traffic 0.1 EXPERIMENTAL native SimConnect test.
echo This bridge has NOT been tested with actual FSLTL models in MSFS 2020.
echo Start MSFS 2020 first, install FSLTL Base Models, disable overlapping traffic.
echo The program will try to create and reposition AI aircraft.
echo Press Ctrl+C to exit and remove owned objects.
set /p ANSWER=Type YES to continue: 
if /I not "%ANSWER%"=="YES" exit /b 1
py -3 -m realflow doctor
py -3 -m realflow run --bridge simconnect --allow-motion
pause
