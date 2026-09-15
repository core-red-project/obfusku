# Obfusku Windows Installer (PowerShell)
# Part of Core Red Project / Sxnnyside Project
# Usage: irm https://raw.githubusercontent.com/core-red-project/obfusku/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "core-red-project/obfusku"
$Version = if ($env:OBFUSKU_VERSION) { $env:OBFUSKU_VERSION } else { "latest" }

if (-not [Environment]::Is64BitOperatingSystem) {
    Write-Error "Obfusku requires a 64-bit Windows operating system."
    exit 1
}

Write-Host "==> Installing Obfusku for Windows (x86_64)..." -ForegroundColor Cyan

# Determine install directory
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { "$HOME\.local\bin" }
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$BaseUrl = if ($Version -eq "latest") {
    "https://github.com/$Repo/releases/latest/download"
} else {
    $Tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
    "https://github.com/$Repo/releases/download/$Tag"
}

$CliUrl = "$BaseUrl/obfusku-windows-x86_64.exe"
$LspUrl = "$BaseUrl/obfusku-lsp-windows-x86_64.exe"

$CliTarget = Join-Path $InstallDir "obfusku.exe"
$LspTarget = Join-Path $InstallDir "obfusku-lsp.exe"

Write-Host "==> Downloading obfusku CLI from $CliUrl..." -ForegroundColor Cyan
Invoke-WebRequest -Uri $CliUrl -OutFile $CliTarget -UseBasicParsing

Write-Host "==> Downloading obfusku-lsp from $LspUrl..." -ForegroundColor Cyan
try {
    Invoke-WebRequest -Uri $LspUrl -OutFile $LspTarget -UseBasicParsing
} catch {
    Write-Warning "Could not download obfusku-lsp (optional). Continuing with CLI only."
}

# Update User PATH if needed
$UserPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
$Paths = $UserPath -split ";"
if ($Paths -notcontains $InstallDir) {
    Write-Host "==> Adding $InstallDir to your User PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", [EnvironmentVariableTarget]::User)
    $env:Path = "$env:Path;$InstallDir"
}

Write-Host "==> Obfusku installed successfully to $CliTarget!" -ForegroundColor Green
try {
    & $CliTarget --version
} catch {
    # Ignored if PATH refresh required
}

Write-Host "`nRestart your terminal or run: & '$CliTarget' --help to get started." -ForegroundColor Green
