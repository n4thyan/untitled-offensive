@echo off
REM UNTITLED local friends-alpha match: [MAP] [GAMETYPE] [BOTCOUNT]
REM Starts the project server and client without writing to the retail install.
setlocal EnableExtensions

set "ROOT=%~dp0"
set "LOGS=%ROOT%dev-logs"
set "CLASSES=%ROOT%assets\classes"
set "OVERLAY=%ROOT%main-untitled"

REM A packaged build keeps the executables beside this script. A source checkout
REM keeps them under target\release and can build them when absent.
if exist "%ROOT%vcod.exe" (
  set "BIN=%ROOT%"
) else (
  set "BIN=%ROOT%target\release\"
)

REM --game-dir wins in the binaries. This launcher honours COD_DIR first, then
REM checks common Windows retail/Steam locations. It never modifies them.
set "GAME_DIR=%COD_DIR%"
if not defined GAME_DIR if exist "C:\Program Files (x86)\Call of Duty\main\pak0.pk3" set "GAME_DIR=C:\Program Files (x86)\Call of Duty"
if not defined GAME_DIR if exist "C:\Program Files (x86)\Steam\steamapps\common\Call of Duty\main\pak0.pk3" set "GAME_DIR=C:\Program Files (x86)\Steam\steamapps\common\Call of Duty"
if not defined GAME_DIR if exist "C:\Program Files\Steam\steamapps\common\Call of Duty\main\pak0.pk3" set "GAME_DIR=C:\Program Files\Steam\steamapps\common\Call of Duty"
if not defined GAME_DIR if exist "D:\Steam\steamapps\common\Call of Duty\main\pak0.pk3" set "GAME_DIR=D:\Steam\steamapps\common\Call of Duty"

set "MAP=%~1"
if "%MAP%"=="" set "MAP=mp_carentan"
set "GAMETYPE=%~2"
if "%GAMETYPE%"=="" set "GAMETYPE=dm"
set "BOTS=%~3"
if "%BOTS%"=="" set "BOTS=3"

if not defined GAME_DIR (
  echo ERROR: Call of Duty was not found.
  echo Set COD_DIR to the folder containing main\pak0.pk3 and retry.
  exit /b 1
)
if not exist "%GAME_DIR%\main\pak0.pk3" (
  echo ERROR: "%GAME_DIR%" does not contain main\pak0.pk3.
  exit /b 1
)

if not exist "%BIN%vcod-server.exe" (
  echo Building release binaries...
  pushd "%ROOT%"
  cargo build --release -p vcod -p vcod-server
  set "BUILD_ERROR=%ERRORLEVEL%"
  popd
  if not "%BUILD_ERROR%"=="0" exit /b %BUILD_ERROR%
)
if not exist "%BIN%vcod.exe" (
  echo ERROR: missing "%BIN%vcod.exe".
  exit /b 1
)

REM The source-only Toccoa map may be rebuilt when its tools are present. It is
REM not part of the friends-alpha package.
if /i "%MAP%"=="mp_toccoa" if exist "%ROOT%tools\toccoa\build_toccoa.py" (
  pushd "%ROOT%tools\toccoa"
  python validate_spawns.py && python build_toccoa.py && python package_pk3.py
  set "MAP_ERROR=%ERRORLEVEL%"
  popd
  if not "%MAP_ERROR%"=="0" exit /b %MAP_ERROR%
)

if not exist "%LOGS%" mkdir "%LOGS%"
set "OVERLAY_ARG="
if exist "%OVERLAY%" set "OVERLAY_ARG=--overlay-dir "%OVERLAY%""

echo.
echo UNTITLED friends-alpha local match
echo map=%MAP%  gametype=%GAMETYPE%  bots=%BOTS%
echo retail=%GAME_DIR%
echo logs=%LOGS%
echo.

start "UNTITLED server" cmd /k ""%BIN%vcod-server.exe" --game-dir "%GAME_DIR%" %OVERLAY_ARG% --gametype %GAMETYPE% --bots %BOTS% --bots-shoot --port 28960 --classes-dir "%CLASSES%" %MAP% ^> "%LOGS%\server.log" 2^>^&1"
echo Waiting for the server...
timeout /t 12 /nobreak >nul
start "UNTITLED client" cmd /c ""%BIN%vcod.exe" --game-dir "%GAME_DIR%" %OVERLAY_ARG% --connect 127.0.0.1:28960 --team allies --weapon m1carbine_mp ^> "%LOGS%\client.log" 2^>^&1"

echo Client launched. Press M for the multiplayer menu.
echo Use stop-dev-match.bat to stop both project processes.
endlocal
