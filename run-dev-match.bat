@echo off
REM ============================================================================
REM  UNTITLED - development match launcher
REM
REM  Starts a dedicated CoD1 server with bots on your retail install and, after
REM  a short warm-up, launches the client connected to it.
REM
REM  The retail Call of Duty install is only ever READ. Nothing is copied into
REM  it, nothing is written to it, and no extracted asset is kept inside this
REM  repository.
REM
REM  Usage:
REM    run-dev-match.bat [MAP] [GAMETYPE] [BOTCOUNT]
REM      MAP        defaults to mp_carentan
REM      GAMETYPE   dm or tdm      defaults to dm
REM      BOTCOUNT   defaults to 3
REM
REM  Example:
REM    run-dev-match.bat mp_carentan dm 4
REM ============================================================================

setlocal

set "GAME_DIR=C:\Program Files (x86)\Call of Duty"

REM Where this script lives; logs go next to the binaries.
set "ROOT=%~dp0"
set "BIN=%ROOT%target\release"
set "LOGS=%ROOT%dev-logs"

set "MAP=%~1"
if "%MAP%"=="" set "MAP=mp_carentan"

set "GAMETYPE=%~2"
if "%GAMETYPE%"=="" set "GAMETYPE=dm"

set "BOTS=%~3"
if "%BOTS%"=="" set "BOTS=3"

if not exist "%LOGS%" mkdir "%LOGS%"

if not exist "%BIN%\vcod-server.exe" (
  echo Building release binaries...
  cargo build --release
  if errorlevel 1 (
    echo BUILD FAILED
    exit /b 1
  )
)

if not exist "%GAME_DIR%\main\pak0.pk3" (
  echo ERROR: Call of Duty install not found at "%GAME_DIR%".
  echo Edit GAME_DIR at the top of this script and retry.
  exit /b 1
)

echo.
echo   UNTITLED dev match
echo   ------------------
echo   map      : %MAP%
echo   gametype : %GAMETYPE%
echo   bots     : %BOTS%
echo   game dir : %GAME_DIR%
echo   logs     : %LOGS%
echo.

REM --- dedicated server -------------------------------------------------------
echo Starting dedicated server on udp/28960 ...
start "UNTITLED server" cmd /k ""%BIN%\vcod-server.exe" --game-dir "%GAME_DIR%" --gametype %GAMETYPE% --bots %BOTS% --bots-shoot --port 28960 %MAP% > "%LOGS%\server.log" 2>&1"

REM The server needs to load the map, parse the gametype script and run the
REM bots through the join menus before a client can see a live match. 12s is
REM comfortably more than that on a normal boot; the client's own connect is
REM retried anyway if the server is not listening yet.
echo Waiting for the server to come up...
timeout /t 12 /nobreak >nul

REM --- client -----------------------------------------------------------------
echo Launching client, joining as allies with the M1 Carbine ...
start "UNTITLED client" "%BIN%\vcod.exe" --game-dir "%GAME_DIR%" --connect 127.0.0.1:28960 --team allies --weapon m1carbine_mp > "%LOGS%\client.log" 2>&1

echo.
echo Client launched.
echo   click the game window to capture the mouse, Esc to release it
echo   WASD move · Space jump · Ctrl crouch · Z prone · Q/E lean · Shift walk
echo   LMB fire · RMB aim · R reload · 1-6 weapons · F3 debug overlay
echo.
echo Server log : %LOGS%\server.log
echo Client log : %LOGS%\client.log
echo.
echo Close the server window, or run stop-dev-match.bat, to shut it down.

endlocal