#!/usr/bin/env bash
# Obfusku Universal Installer
# Part of Core Red Project / Sxnnyside Project
# https://github.com/core-red-project/obfusku

set -euo pipefail

REPO="core-red-project/obfusku"
BINARY_NAME="obfusku"

# Colors for terminal output
BOLD="$(tput bold 2>/dev/null || echo '')"
GREEN="$(tput setaf 2 2>/dev/null || echo '')"
CYAN="$(tput setaf 6 2>/dev/null || echo '')"
RED="$(tput setaf 1 2>/dev/null || echo '')"
YELLOW="$(tput setaf 3 2>/dev/null || echo '')"
RESET="$(tput sgr0 2>/dev/null || echo '')"

info() {
    printf "${CYAN}==>${RESET} ${BOLD}%s${RESET}\n" "$1"
}

success() {
    printf "${GREEN}==>${RESET} ${BOLD}%s${RESET}\n" "$1"
}

warn() {
    printf "${YELLOW}WARNING:${RESET} %s\n" "$1"
}

error() {
    printf "${RED}ERROR:${RESET} %s\n" "$1" >&2
    exit 1
}

# 1. Detect OS
OS_RAW="$(uname -s)"
case "$OS_RAW" in
    Linux*)     OS="linux" ;;
    Darwin*)    OS="macos" ;;
    MINGW*|MSYS*|CYGWIN*)
        error "Windows detected. Please run the Windows PowerShell installer instead:\n  irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex"
        ;;
    *)          error "Unsupported operating system: $OS_RAW. Obfusku supports Linux, macOS, and Windows." ;;
esac

# 2. Detect Architecture
ARCH_RAW="$(uname -m)"
case "$ARCH_RAW" in
    x86_64|amd64)   ARCH="x86_64" ;;
    arm64|aarch64)  ARCH="aarch64" ;;
    *)              error "Unsupported CPU architecture: $ARCH_RAW. Supported architectures: x86_64, aarch64." ;;
esac

# 3. Match release artifacts
if [ "$OS" = "linux" ] && [ "$ARCH" = "x86_64" ]; then
    ARTIFACT_NAME="obfusku-linux-x86_64"
    LSP_ARTIFACT_NAME="obfusku-lsp-linux-x86_64"
elif [ "$OS" = "macos" ] && [ "$ARCH" = "aarch64" ]; then
    ARTIFACT_NAME="obfusku-macos-aarch64"
    LSP_ARTIFACT_NAME="obfusku-lsp-macos-aarch64"
elif [ "$OS" = "linux" ] && [ "$ARCH" = "aarch64" ]; then
    error "Pre-built Linux ARM64 binary is not currently distributed. You can install via Cargo: cargo install --git https://github.com/$REPO.git obfusku-cli"
else
    error "No pre-built binary available for $OS-$ARCH."
fi

# 4. Resolve download URL
VERSION="${OBFUSKU_VERSION:-latest}"
if [ "$VERSION" = "latest" ]; then
    DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$ARTIFACT_NAME"
else
    # Normalize version with 'v' prefix
    case "$VERSION" in
        v*) TAG="$VERSION" ;;
        *)  TAG="v$VERSION" ;;
    esac
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$ARTIFACT_NAME"
fi

# 5. Determine installation directory
if [ -n "${INSTALL_DIR:-}" ]; then
    TARGET_DIR="$INSTALL_DIR"
    USE_SUDO=0
elif [ -w "/usr/local/bin" ]; then
    TARGET_DIR="/usr/local/bin"
    USE_SUDO=0
elif command -v sudo >/dev/null 2>&1 && [ -d "/usr/local/bin" ]; then
    TARGET_DIR="/usr/local/bin"
    USE_SUDO=1
else
    TARGET_DIR="$HOME/.local/bin"
    USE_SUDO=0
fi

info "Installing Obfusku ($OS-$ARCH)..."
info "Downloading binary from $DOWNLOAD_URL"

TMP_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t 'obfusku-install')"
trap 'rm -rf "$TMP_DIR"' EXIT

TMP_BIN="$TMP_DIR/$BINARY_NAME"

if command -v curl >/dev/null 2>&1; then
    curl -fSL "$DOWNLOAD_URL" -o "$TMP_BIN"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$TMP_BIN" "$DOWNLOAD_URL"
else
    error "Neither curl nor wget was found. Please install either tool to continue."
fi

chmod +x "$TMP_BIN"

# Ensure destination exists
if [ ! -d "$TARGET_DIR" ]; then
    mkdir -p "$TARGET_DIR"
fi

info "Placing binary in $TARGET_DIR/$BINARY_NAME"
if [ "$USE_SUDO" -eq 1 ]; then
    sudo install -m 755 "$TMP_BIN" "$TARGET_DIR/$BINARY_NAME"
else
    install -m 755 "$TMP_BIN" "$TARGET_DIR/$BINARY_NAME"
fi

# Also attempt to download and install obfusku-lsp
if [ -n "${LSP_ARTIFACT_NAME:-}" ]; then
    LSP_URL="${DOWNLOAD_URL%/*}/$LSP_ARTIFACT_NAME"
    TMP_LSP="$TMP_DIR/obfusku-lsp"
    if curl -fSL "$LSP_URL" -o "$TMP_LSP" 2>/dev/null; then
        chmod +x "$TMP_LSP"
        if [ "$USE_SUDO" -eq 1 ]; then
            sudo install -m 755 "$TMP_LSP" "$TARGET_DIR/obfusku-lsp"
        else
            install -m 755 "$TMP_LSP" "$TARGET_DIR/obfusku-lsp"
        fi
        info "Installed Language Server: $TARGET_DIR/obfusku-lsp"
    fi
fi

# 6. Verify installation
INSTALLED_BIN="$TARGET_DIR/$BINARY_NAME"
if [ -x "$INSTALLED_BIN" ]; then
    VERSION_OUTPUT="$("$INSTALLED_BIN" --version 2>/dev/null || echo 'installed')"
    success "Obfusku successfully installed! ($VERSION_OUTPUT)"
else
    error "Installation verification failed at $INSTALLED_BIN."
fi

# 7. Check PATH
case ":$PATH:" in
    *":$TARGET_DIR:"*) ;;
    *)
        warn "$TARGET_DIR is not in your PATH."
        printf "Add it by running:\n  export PATH=\"%s:\$PATH\"\n" "$TARGET_DIR"
        ;;
esac

printf "\nRun '${BOLD}obfusku --help${RESET}' to get started.\n"
