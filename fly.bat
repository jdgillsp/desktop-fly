@echo off
setlocal EnableDelayedExpansion
rem DesktopFly launcher (Windows / Rust build).
rem PowerShell does not search the current directory, so run it as .\fly.bat
rem   fly.bat                       build + run the fly
rem   fly.bat --creature salticid   any dfshell flag is passed straight through
rem   fly.bat --habitat --literal
rem   fly.bat -n                    skip the build, just run the last binary
rem
rem Builds into rust\target-web so that `cargo test` / `cargo build` in
rem rust\target stay untouched, and runs a *copy* of the exe -- a running fly
rem therefore never holds the link target, so a rebuild while it is up works.

set "ROOT=%~dp0"
set "ROOT=%ROOT:~0,-1%"
set "TARGET=%ROOT%\rust\target-web"
set "RUNDIR=%TARGET%\run"
set "EXE=%RUNDIR%\desktopfly.exe"

set "BUILD=1"
set "ARGS="
for %%A in (%*) do (
  if "%%~A"=="-n" (
    set "BUILD=0"
  ) else if "%%~A"=="--no-build" (
    set "BUILD=0"
  ) else (
    set "ARGS=!ARGS! %%A"
  )
)

if "%BUILD%"=="1" (
  set "CARGO=cargo"
  where cargo >nul 2>&1 || set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
  if not exist "!CARGO!" if "!CARGO!" neq "cargo" (
    echo fly.bat: cargo not found on PATH or in %%USERPROFILE%%\.cargo\bin
    exit /b 1
  )
  echo === building dfshell into rust\target-web ===
  set "CARGO_TARGET_DIR=%TARGET%"
  pushd "%ROOT%\rust"
  "!CARGO!" build --release -p dfshell
  set "RC=!ERRORLEVEL!"
  popd
  if not "!RC!"=="0" exit /b !RC!
  if not exist "%RUNDIR%" mkdir "%RUNDIR%"
  copy /y "%TARGET%\release\desktopfly.exe" "%EXE%" >nul || exit /b 1
)

if not exist "%EXE%" (
  echo fly.bat: %EXE% missing -- run without -n to build it first.
  exit /b 1
)

rem Pin the connectome explicitly; the copied exe sits deeper than data\.
set "DESKTOPFLY_DATA=%ROOT%\data"
echo === running desktopfly%ARGS% ===
"%EXE%"%ARGS%
