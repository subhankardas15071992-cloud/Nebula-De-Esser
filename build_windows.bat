@echo off
REM ─────────────────────────────────────────────────────────────────────────────
REM Nebula DeEsser — Windows Build Script
REM Builds CLAP plugin optimized for ASIO / WASAPI / WaveRT
REM Requires: Rust stable toolchain, MSVC or GNU toolchain
REM ─────────────────────────────────────────────────────────────────────────────
setlocal enabledelayedexpansion

echo ╔════════════════════════════════════════════════╗
echo ║  NEBULA DEESSER - Windows Build                ║
echo ╚════════════════════════════════════════════════╝
echo.

REM Check for cargo
where cargo >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [ERROR] cargo not found. Install Rust from https://rustup.rs
    pause
    exit /b 1
)

REM Install nih-plug bundler if not present
cargo nih-plug --help >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [*] Installing cargo-nih-plug bundler...
    cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
)

echo [*] Building release binary...
set RUSTFLAGS=-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1

cargo build --release
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Build failed!
    pause
    exit /b 1
)

echo [*] Bundling CLAP plugin...
cargo nih-plug bundle nebula_desser --release
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Bundle failed!
    pause
    exit /b 1
)

echo.
echo [OK] Build complete!
echo.

REM Find CLAP file
for /r "target" %%f in (*.clap) do (
    echo [OK] CLAP: %%f
    echo.
    echo To install, copy to:
    echo   %%COMMONPROGRAMFILES%%\CLAP\
    echo   or
    echo   %%LOCALAPPDATA%%\Programs\Common\CLAP\
    echo.
    echo For ASIO: Use ASIO4ALL or your audio interface's ASIO driver.
    echo For WASAPI exclusive mode: Enable in your DAW's audio settings.
    echo For WaveRT: Automatically used by Windows audio stack for low latency.
    goto :done
)

echo [!] CLAP file not found. Check build output.

:done
echo.
pause
endlocal
