# ─────────────────────────────────────────────────────────────────────────────
# Nebula DeEsser — Windows PowerShell Build Script
# Builds CLAP plugin optimized for ASIO / WASAPI / WaveRT
# ─────────────────────────────────────────────────────────────────────────────

$ErrorActionPreference = "Stop"

Write-Host "╔════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  NEBULA DEESSER - Windows Build (PowerShell)   ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# Check for cargo
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "[ERROR] cargo not found. Install Rust from https://rustup.rs" -ForegroundColor Red
    exit 1
}

# Install nih-plug bundler
try {
    $null = & cargo nih-plug --help 2>&1
} catch {
    Write-Host "[*] Installing cargo-nih-plug bundler..." -ForegroundColor Yellow
    & cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
}

Write-Host "[*] Building release binary..." -ForegroundColor Green
$env:RUSTFLAGS = "-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1"

& cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "[*] Bundling CLAP plugin..." -ForegroundColor Green
& cargo nih-plug bundle nebula_desser --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] Bundle failed!" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "[✓] Build complete!" -ForegroundColor Green
Write-Host ""

# Find and display output
$clapFiles = Get-ChildItem -Path "target" -Filter "*.clap" -Recurse
foreach ($clap in $clapFiles) {
    Write-Host "[✓] CLAP: $($clap.FullName)" -ForegroundColor Cyan

    # Offer to install
    Write-Host ""
    $clapDir = "$env:COMMONPROGRAMFILES\CLAP"
    Write-Host "Install to system CLAP folder:" -ForegroundColor Yellow
    Write-Host "  New-Item -ItemType Directory -Force -Path '$clapDir'" -ForegroundColor Gray
    Write-Host "  Copy-Item '$($clap.FullName)' '$clapDir\'" -ForegroundColor Gray
}

Write-Host ""
Write-Host "ASIO optimization tips:" -ForegroundColor Yellow
Write-Host "  - Use your audio interface's native ASIO driver for lowest latency" -ForegroundColor Gray
Write-Host "  - Set buffer size to 64-256 samples in your DAW" -ForegroundColor Gray
Write-Host "  - For WASAPI exclusive mode: enable in DAW audio settings" -ForegroundColor Gray
Write-Host "  - WaveRT is used automatically by the Windows audio stack" -ForegroundColor Gray
