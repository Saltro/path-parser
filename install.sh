#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────
# path-parser — install / upgrade script
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Saltro/path-parser/master/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- --pre        # use the "latest" pre-release
#   curl -fsSL .../install.sh | bash -s -- --version v0.1.0
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────
REPO="Saltro/path-parser"
INSTALL_DIR="${HOME}/.local/bin"
BINARY="path-parser"

# ── Argument parsing ─────────────────────────────────────────────────
USE_PRE=false
VERSION_TAG=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --pre)
            USE_PRE=true
            shift
            ;;
        --version)
            VERSION_TAG="$2"
            shift 2
            ;;
        --version=*)
            VERSION_TAG="${1#*=}"
            shift
            ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Usage: install.sh [--pre] [--version TAG]" >&2
            exit 1
            ;;
    esac
done

# ── Helpers ──────────────────────────────────────────────────────────
info()  { printf "\033[1;36m==>\033[0m %s\n" "$*"; }
warn()  { printf "\033[1;33m==>\033[0m %s\n" "$*" >&2; }
error() { printf "\033[1;31m==>\033[0m %s\n" "$*" >&2; exit 1; }

require_cmd() {
    command -v "$1" &>/dev/null || error "Required command not found: $1"
}

# ── Platform detection ───────────────────────────────────────────────
detect_target() {
    local os arch
    case "$(uname -s)" in
        Darwin)  os="apple-darwin" ;;
        Linux)   os="unknown-linux-gnu" ;;
        *)       error "Unsupported OS: $(uname -s). Only macOS and Linux are supported." ;;
    esac
    case "$(uname -m)" in
        arm64|aarch64)   arch="aarch64" ;;
        x86_64|amd64)    arch="x86_64" ;;
        *)               error "Unsupported architecture: $(uname -m)" ;;
    esac
    echo "${arch}-${os}"
}

# ── Resolve the release tag and version ──────────────────────────────
#
# Strategy (avoids the GitHub REST API rate limit for the common case):
#
#   1. Stable (default):  follow /releases/latest redirect → tag "v0.1.0"
#                         version is derived from the tag: strip leading "v"
#
#   2. --pre ("latest"):  fetch Cargo.toml from raw.githubusercontent.com
#                         to learn the current version, then download
#                         directly from the "latest" tag.
#
#   3. --version TAG:     if TAG starts with "v", version = TAG[1:];
#                         otherwise treat like --pre but for that tag.
#
# Sets global vars: RELEASE_TAG, RELEASE_VERSION
resolve_release() {
    if [[ -n "$VERSION_TAG" ]]; then
        RELEASE_TAG="$VERSION_TAG"
    elif $USE_PRE; then
        RELEASE_TAG="latest"
    else
        RELEASE_TAG=""
    fi

    # ── Case 1: we need to discover the latest stable tag ────────
    if [[ -z "$RELEASE_TAG" ]]; then
        info "Resolving latest stable release..."
        local redirect
        redirect=$(curl -sI -o /dev/null -w '%{redirect_url}' \
            "https://github.com/${REPO}/releases/latest")
        if [[ -z "$redirect" || "$redirect" == "null" ]]; then
            # No stable release yet — fall back to pre-release.
            warn "No stable release found. Falling back to 'latest' pre-release..."
            RELEASE_TAG="latest"
        else
            RELEASE_TAG=$(basename "$redirect")
        fi
    fi

    # ── Case 2: tag is a version (starts with "v") ───────────────
    if [[ "$RELEASE_TAG" == v* ]]; then
        RELEASE_VERSION="${RELEASE_TAG#v}"
        return 0
    fi

    # ── Case 3: non-version tag ("latest" or custom) ─────────────
    # Read version from Cargo.toml on the default branch.
    info "Reading version from Cargo.toml (tag: ${RELEASE_TAG})..."
    local cargo_toml
    cargo_toml=$(curl -fsSL \
        "https://raw.githubusercontent.com/${REPO}/master/Cargo.toml" 2>/dev/null) \
        || error "Could not fetch Cargo.toml. Is the repo public?"
    RELEASE_VERSION=$(echo "$cargo_toml" | grep '^version' | head -1 \
        | sed 's/.*= *"\(.*\)".*/\1/')
    [[ -n "$RELEASE_VERSION" ]] || error "Could not parse version from Cargo.toml"
}

# ── Build the direct download URL ────────────────────────────────────
#
# GitHub serves release assets at:
#   https://github.com/{owner}/{repo}/releases/download/{tag}/{filename}
#
# Our CI names assets: path-parser-v{version}-{target}.tar.gz
build_download_url() {
    local tag="$1" version="$2" target="$3"
    local filename="${BINARY}-v${version}-${target}.tar.gz"
    echo "https://github.com/${REPO}/releases/download/${tag}/${filename}"
}

# ── Verify the URL exists (avoid downloading a 404 HTML page) ────────
check_url() {
    local url="$1"
    local http_code
    http_code=$(curl -sI -o /dev/null -w '%{http_code}' -L "$url")
    [[ "$http_code" == "200" ]]
}

# ── Main ─────────────────────────────────────────────────────────────
main() {
    require_cmd curl
    require_cmd tar

    local target
    target=$(detect_target)
    info "Detected platform: ${target}"

    # Resolve which release to install.
    resolve_release
    local display_name="${RELEASE_TAG}"
    [[ "$RELEASE_TAG" == "latest" ]] && display_name="latest (pre-release)"
    info "Release: ${display_name}  (version ${RELEASE_VERSION})"

    # Build download URL and verify the asset exists.
    local url
    url=$(build_download_url "$RELEASE_TAG" "$RELEASE_VERSION" "$target")
    local filename
    filename=$(basename "$url")

    if ! check_url "$url"; then
        error "No asset found: ${filename}"$'\n'"$'\n'Check available releases at: https://github.com/${REPO}/releases"
    fi

    # Download.
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "${TMP_DIR}"' EXIT

    local archive="${TMP_DIR}/archive.tar.gz"
    info "Downloading ${filename}..."
    curl -fSL --progress-bar -o "${archive}" "$url"

    # Extract.
    info "Extracting..."
    tar -xzf "${archive}" -C "${TMP_DIR}"

    # Find the extracted binary.
    local bin_src
    bin_src=$(find "${TMP_DIR}" -type f -name "${BINARY}" | head -1)
    [[ -n "$bin_src" ]] || error "Could not find ${BINARY} in the archive."

    # Install.
    mkdir -p "${INSTALL_DIR}"
    local dest="${INSTALL_DIR}/${BINARY}"

    if [[ -f "$dest" ]]; then
        local old_ver
        old_ver=$("$dest" --version 2>/dev/null || echo "unknown")
        info "Upgrading ${old_ver} → v${RELEASE_VERSION}"
    else
        info "Installing to ${dest}"
    fi

    cp "${bin_src}" "${dest}"
    chmod +x "${dest}"

    info "Done! ${BINARY} installed at ${dest}"

    # PATH check.
    if ! echo ":${PATH}:" | grep -q ":${INSTALL_DIR}:"; then
        warn "${INSTALL_DIR} is not in your PATH."
        echo ""
        echo "   Add this line to your shell config (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish):"
        echo ""
        echo "       export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
    fi

    echo ""
    info "Run '${BINARY}' to start, or '${BINARY} upgrade' to update later."
}

main "$@"
