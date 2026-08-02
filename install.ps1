# Install MovieBox-TUI (wakeupbrk fork) via cargo.
# Usage: irm https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/install.ps1 | iex
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Info { param([string]$Message) Write-Host "==> $Message" -ForegroundColor Cyan }
function Write-Success { param([string]$Message) Write-Host "==> $Message" -ForegroundColor Green }
function Write-Warn { param([string]$Message) Write-Host "WARN: $Message" -ForegroundColor Yellow }
function Write-Err { param([string]$Message) Write-Host "ERROR: $Message" -ForegroundColor Red; exit 1 }

$Repo = "https://github.com/wakeupbrk/MovieBox-Tui.git"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Err "Rust/cargo not found. Install from https://rustup.rs then re-run this script."
}

Write-Info "Installing MovieBox-TUI from $Repo …"
cargo install --git $Repo --locked --force
if ($LASTEXITCODE -ne 0) {
    Write-Err "cargo install failed."
}

Write-Success "Done. Run: moviebox-tui"
Write-Host ""
Write-Host "  Sources: Ctrl+P   Continue: Ctrl+W   Library: Ctrl+Z   Help: ?"
Write-Host "  Repo:    https://github.com/wakeupbrk/MovieBox-Tui"
Write-Host ""
