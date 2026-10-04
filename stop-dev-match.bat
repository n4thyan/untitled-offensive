@echo off
REM Stops the UNTITLED dev match: the dedicated server and any client left
REM running. Nothing else is touched, and no files are deleted.
taskkill /F /IM vcod-server.exe >nul 2>&1
taskkill /F /IM vcod.exe >nul 2>&1
echo UNTITLED dev match stopped.