@echo off
chcp 65001 >nul
"%~dp0python\python.exe" "%~dp0start.pyw" --doctor
echo.
pause
