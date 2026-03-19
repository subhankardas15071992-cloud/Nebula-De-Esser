@echo off
REM ─────────────────────────────────────────────────────────────────────────────
REM Nebula DeEsser v2.0.0 — Windows Build Script
REM Builds 64-bit CLAP plugin optimized for ASIO / WASAPI / WaveRT
REM Requires: Rust stable toolchain + MSVC (Visual Studio 2022) or GNU
REM ─────────────────────────────────────────────────────────────────────────────
setlocal enabledelayedexpansion

echo ╔════════════════════════════════════════════════╗
echo ║  NEBULA DEESSER v2.0.0 — Windows Build         ║
echo ╚════════════════════════════════════════════════╝
echo.

REM Ensure 64-bit target
if "%PROCESSOR_ARCHITECTURE%"=="x86" (
    if "%PROCESSOR_ARCHITEW6432%"=="" (
        echo [ERROR] 32-bit system detected. A 64-bit OS is required.
        pause & exit /b 1
    )
)

REM Check for cargo
where cargo >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [ERROR] cargo not found. Install Rust from https://rustup.rs
    pause & exit /b 1
)

REM Verify 64-bit Rust target is present
rustup target list --installed | find "x86_64-pc-windows" >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [*] Adding x86_64-pc-windows-msvc target...
    rustup target add x86_64-pc-windows-msvc
)

REM Install nih-plug bundler if not present
cargo nih-plug --help >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [*] Installing cargo-nih-plug bundler...
    cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
)

echo [*] Building release binary (64-bit, LTO, max opt)...
set RUSTFLAGS=-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1

cargo build --release
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Build failed! See output above.
    pause & exit /b 1
)

echo [*] Bundling CLAP plugin...
cargo nih-plug bundle nebula_desser --release
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Bundle failed!
    pause & exit /b 1
)

echo.
echo [OK] Build complete! — Nebula DeEsser v2.0.0
echo.

REM Locate and report CLAP output
for /r "target" %%f in (*.clap) do (
    echo [OK] CLAP: %%f
    echo.
    echo Install by copying to one of:
    echo   %%COMMONPROGRAMFILES%%\CLAP\
    echo   %%LOCALAPPDATA%%\Programs\Common\CLAP\
    echo.
    echo Audio backend tips:
    echo   ASIO         : Use ASIO4ALL or your interface driver for ~1ms latency
    echo   WASAPI Excl. : Enable Exclusive Mode in your DAW audio settings
    echo   WaveRT       : Used automatically by Windows audio stack
    echo.
    echo New in v2.0.0:
    echo   - Presets: save/load envelope settings
    echo   - Undo / Redo (50-step history)
    echo   - MIDI Learn for all main parameters
    echo   - FX Bypass (soft bypass)
    echo   - Input / Output Level + Pan knobs
    echo   - Oversampling: Off / 2x / 4x / 6x / 8x
    echo   - Fixed spectrum analyzer (live FFT display)
    goto :done
)

echo [!] CLAP file not found in target\. Check build output.
goto :done

:done
echo.
pause
endlocal
