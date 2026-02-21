#!/bin/sh
# swe-forge installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/CortexLM/swe-forge/main/install.sh | sh
#
# Environment variables:
#   SWE_FORGE_VERSION  - Install a specific version (e.g. "v1.0.0"). Default: latest
#   SWE_FORGE_INSTALL  - Installation directory. Default: ~/.swe-forge/bin (or /usr/local/bin as root)
#
# This script:
#   1. Detects your OS and architecture
#   2. Downloads the correct binary from GitHub Releases
#   3. Installs it to the target directory
#   4. Adds the directory to PATH if needed
#   5. Re-running this script will upgrade an existing installation

set -eu

REPO="CortexLM/swe-forge"
BINARY_NAME="swe-forge"

main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    # Detect OS
    _os="$(uname -s)"
    case "$_os" in
        Linux)  os="linux" ;;
        Darwin)
            err "macOS is not yet supported. Please build from source:
  git clone https://github.com/${REPO}.git && cd swe-forge && cargo build --release"
            ;;
        *)
            err "Unsupported operating system: $_os"
            ;;
    esac

    # Detect architecture
    _arch="$(uname -m)"
    case "$_arch" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)   arch="aarch64" ;;
        *)
            err "Unsupported architecture: $_arch"
            ;;
    esac

    # Determine version
    if [ -n "${SWE_FORGE_VERSION:-}" ]; then
        version="$SWE_FORGE_VERSION"
        # Ensure version starts with 'v'
        case "$version" in
            v*) ;;
            *)  version="v${version}" ;;
        esac
        tag="$version"
    else
        tag="latest"
        version="latest"
    fi

    # Determine install directory
    if [ -n "${SWE_FORGE_INSTALL:-}" ]; then
        install_dir="$SWE_FORGE_INSTALL"
    elif [ "$(id -u)" = "0" ]; then
        install_dir="/usr/local/bin"
    else
        install_dir="$HOME/.swe-forge/bin"
    fi

    info "swe-forge installer"
    info "  OS:           $os"
    info "  Architecture: $arch"
    info "  Version:      $version"
    info "  Install to:   $install_dir"
    info ""

    # Build download URL
    if [ "$tag" = "latest" ]; then
        download_url="https://github.com/${REPO}/releases/latest/download/${BINARY_NAME}-latest-${os}-${arch}.tar.gz"
    else
        download_url="https://github.com/${REPO}/releases/download/${tag}/${BINARY_NAME}-${tag}-${os}-${arch}.tar.gz"
    fi

    # Create temp directory
    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "$tmp_dir"' EXIT

    info "Downloading ${BINARY_NAME}..."
    info "  URL: $download_url"

    # Download
    http_code=$(curl -fsSL -w '%{http_code}' -o "$tmp_dir/archive.tar.gz" "$download_url" 2>/dev/null) || true

    if [ ! -f "$tmp_dir/archive.tar.gz" ] || [ "${http_code:-0}" -ge 400 ]; then
        # If latest tag doesn't exist yet, try downloading from the most recent tagged release
        if [ "$tag" = "latest" ]; then
            info "No 'latest' release found, checking for most recent tagged release..."
            latest_tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
            if [ -n "$latest_tag" ]; then
                download_url="https://github.com/${REPO}/releases/download/${latest_tag}/${BINARY_NAME}-${latest_tag}-${os}-${arch}.tar.gz"
                info "  Trying: $download_url"
                curl -fsSL -o "$tmp_dir/archive.tar.gz" "$download_url" || \
                    err "Download failed. No release binaries found.

Build from source instead:
  git clone https://github.com/${REPO}.git && cd swe-forge && cargo build --release"
            else
                err "No releases found for ${REPO}.

Build from source instead:
  git clone https://github.com/${REPO}.git && cd swe-forge && cargo build --release"
            fi
        else
            err "Download failed for version ${tag}.

Check available versions at: https://github.com/${REPO}/releases

Build from source instead:
  git clone https://github.com/${REPO}.git && cd swe-forge && cargo build --release"
        fi
    fi

    # Extract
    info "Extracting..."
    tar xzf "$tmp_dir/archive.tar.gz" -C "$tmp_dir"

    if [ ! -f "$tmp_dir/${BINARY_NAME}" ]; then
        err "Binary not found in archive. The release may be corrupted."
    fi

    # Install
    mkdir -p "$install_dir"

    # Check if we're upgrading
    if [ -f "${install_dir}/${BINARY_NAME}" ]; then
        old_version=$("${install_dir}/${BINARY_NAME}" --version 2>/dev/null | awk '{print $NF}' || echo "unknown")
        info "Upgrading from ${old_version}..."
    fi

    cp "$tmp_dir/${BINARY_NAME}" "${install_dir}/${BINARY_NAME}"
    chmod +x "${install_dir}/${BINARY_NAME}"

    info "Installed ${BINARY_NAME} to ${install_dir}/${BINARY_NAME}"

    # Verify installation
    installed_version=$("${install_dir}/${BINARY_NAME}" --version 2>/dev/null | awk '{print $NF}' || echo "unknown")
    info "Version: ${installed_version}"

    # Check PATH
    case ":${PATH}:" in
        *":${install_dir}:"*)
            ;;
        *)
            info ""
            warn "${install_dir} is not in your PATH."
            info ""
            info "Add it by running:"
            info ""

            # Detect shell
            _shell="$(basename "${SHELL:-/bin/sh}")"
            case "$_shell" in
                zsh)
                    info "  echo 'export PATH=\"${install_dir}:\$PATH\"' >> ~/.zshrc"
                    info "  source ~/.zshrc"
                    ;;
                fish)
                    info "  fish_add_path ${install_dir}"
                    ;;
                *)
                    info "  echo 'export PATH=\"${install_dir}:\$PATH\"' >> ~/.bashrc"
                    info "  source ~/.bashrc"
                    ;;
            esac
            info ""
            ;;
    esac

    info ""
    info "Done! Run 'swe-forge --help' to get started."
    info ""
    info "To update later, run:"
    info "  swe-forge self-update"
    info ""
    info "Or re-run this installer:"
    info "  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | sh"
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "Required command not found: $1"
    fi
}

info() {
    printf '  \033[1;32m%s\033[0m\n' "$*"
}

warn() {
    printf '  \033[1;33mwarning:\033[0m %s\n' "$*"
}

err() {
    printf '  \033[1;31merror:\033[0m %s\n' "$*" >&2
    exit 1
}

main "$@"
