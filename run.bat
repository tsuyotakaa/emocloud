@echo off
cd /d "%~dp0"

if exist "emocloud.exe" (
  start "" "emocloud.exe"
  exit /b
)

if exist "target\release\emocloud.exe" (
  start "" "target\release\emocloud.exe"
  exit /b
)

if exist "%LOCALAPPDATA%\emocloud-target\release\emocloud.exe" (
  start "" "%LOCALAPPDATA%\emocloud-target\release\emocloud.exe"
  exit /b
)

echo.
echo emocloud.exe ne nayden.
echo Sobrat:  cargo build --release
echo.
pause
