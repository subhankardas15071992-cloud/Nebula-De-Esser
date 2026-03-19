# ─────────────────────────────────────────────────────────────────────────────
# Nebula DeEsser v2.0.0 — Windows PowerShell Build Script
# Builds 64-bit CLAP plugin optimized for ASIO / WASAPI / WaveRT
# Requires: Rust stable, MSVC (Visual Studio 2022) or GNU toolchain
# ─────────────────────────────────────────────────────────────────────────────

$ErrorActionPreference = "Stop"
$PluginName    = "nebula_desser"
$PluginVersion = "2.0.0"

Write-Host ""
Write-Host "╔════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  NEBULA DEESSER v2.0.0 — Windows Build (PS)    ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# ── Verify 64-bit OS ────────────────────────────────────────────────────────
if (-not [Environment]::Is64BitOperatingSystem) {
    Write-Error "[ERROR] 64-bit Windows is required."
    exit 1
}
Write-Host "[✓] 64-bit OS confirmed" -ForegroundColor Green

# ── Check cargo ─────────────────────────────────────────────────────────────
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "[ERROR] cargo not found. Install Rust from https://rustup.rs"
    exit 1
}
$rustVer = (rustc --version)
Write-Host "[✓] Rust: $rustVer" -ForegroundColor Green

# ── Ensure x86_64 Windows target ────────────────────────────────────────────
Write-Host "[*] Ensuring x86_64-pc-windows-msvc target is installed..."
rustup target add x86_64-pc-windows-msvc 2>$null
Write-Host "[✓] Target ready" -ForegroundColor Green

# ── Install nih-plug bundler if missing ─────────────────────────────────────
$nihPlugOk = $false
try { cargo nih-plug --help 2>$null | Out-Null; $nihPlugOk = $true } catch {}
if (-not $nihPlugOk) {
    Write-Host "[*] Installing cargo-nih-plug bundler..."
    cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
    if ($LASTEXITCODE -ne 0) { Write-Error "[ERROR] Failed to install cargo-nih-plug"; exit 1 }
}
Write-Host "[✓] cargo-nih-plug ready" -ForegroundColor Green

# ── Build ────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "[*] Building release binary (64-bit, LTO, max optimisation)..."
$env:RUSTFLAGS = "-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1"
cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Error "[ERROR] cargo build failed"; exit 1 }
Write-Host "[✓] Build succeeded" -ForegroundColor Green

# ── Bundle ───────────────────────────────────────────────────────────────────
Write-Host "[*] Bundling CLAP plugin..."
cargo nih-plug bundle $PluginName --release
if ($LASTEXITCODE -ne 0) { Write-Error "[ERROR] nih-plug bundle failed"; exit 1 }
Write-Host "[✓] Bundle succeeded" -ForegroundColor Green

# ── Locate output ────────────────────────────────────────────────────────────
Write-Host ""
$clapFile = Get-ChildItem -Path "target" -Filter "*.clap" -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
if ($clapFile) {
    $sizeMB = [math]::Round($clapFile.Length / 1MB, 2)
    Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Green
    Write-Host "║  SUCCESS — Nebula DeEsser v$PluginVersion                         ║" -ForegroundColor Green
    Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Green
    Write-Host ""
    Write-Host "[✓] CLAP : $($clapFile.FullName)" -ForegroundColor Cyan
    Write-Host "[✓] Size : ${sizeMB} MB" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Install by copying to:" -ForegroundColor Yellow
    Write-Host "  $env:COMMONPROGRAMFILES\CLAP\" -ForegroundColor White
    Write-Host "  — or —"
    Write-Host "  $env:LOCALAPPDATA\Programs\Common\CLAP\" -ForegroundColor White
    Write-Host ""

    $installDest = "$env:COMMONPROGRAMFILES\CLAP"
    $doInstall = Read-Host "Install now to $installDest ? [y/N]"
    if ($doInstall -eq 'y' -or $doInstall -eq 'Y') {
        if (-not (Test-Path $installDest)) { New-Item -ItemType Directory -Force -Path $installDest | Out-Null }
        Copy-Item $clapFile.FullName -Destination $installDest -Force
        Write-Host "[✓] Installed to $installDest" -ForegroundColor Green
    }
} else {
    Write-Error "[ERROR] CLAP file not found in target\. Check build output."
    exit 1
}

Write-Host ""
Write-Host "─── Audio backend tips ──────────────────────────────────────────────" -ForegroundColor DarkCyan
Write-Host "  ASIO         : Use your interface driver or ASIO4ALL for ~1ms latency"
Write-Host "  WASAPI Excl. : Enable Exclusive Mode in your DAW audio settings"
Write-Host "  WaveRT       : Used automatically for low-latency kernel streaming"
Write-Host ""
Write-Host "─── New in v2.0.0 ───────────────────────────────────────────────────" -ForegroundColor DarkMagenta
Write-Host "  • Presets: save/load envelope settings"
Write-Host "  • Undo / Redo (50-step history)"
Write-Host "  • MIDI Learn for all main parameters"
Write-Host "  • FX Bypass (soft bypass — title bar turns red)"
Write-Host "  • Input / Output Level + Pan knobs"
Write-Host "  • Oversampling: Off / 2x / 4x / 6x / 8x"
Write-Host "  • Fixed spectrum analyzer (live FFT with smoothing)"
Write-Host ""
