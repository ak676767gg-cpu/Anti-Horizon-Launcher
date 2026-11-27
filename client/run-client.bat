@echo off
rem Starts Minecraft 1.21.11 with the Anti-Horizon client loaded, in offline dev mode, for testing on this PC.
rem Right Shift opens the client menu on any screen. Edit the code, save, run this again to see the change.
if not defined JAVA_HOME set "JAVA_HOME=%USERPROFILE%\tools\jdk-25.0.5.1+2"
cd /d "%~dp0"
call gradlew.bat runClient
