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
INSTALL_URL="https://raw.githubusercontent.com/${REPO}/master/install.sh"

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

# ── Fetch release info ───────────────────────────────────────────────
# Returns the full JSON for a single release.
fetch_release() {
    local tag="$1"
    if [[ -n "$tag" ]]; then
        # Specific tag (includes "latest" as a tag name).
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/tags/${tag}"
    else
        # Latest non-prerelease release.
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest"
    fi
}

# ── Pick the right asset from a release JSON ─────────────────────────
find_asset_url() {
    local json="$1" target="$2"
    # Asset names look like: path-parser-v0.1.0-aarch64-apple-darwin.tar.gz
    local name
    name=$(echo "$json" | grep -oE "\"name\": *\"${BINARY}-v[^\"]+-${target}\\.tar\\.gz\"" | head -1 | sed -E 's/.*"name": *"([^"]+)".*/\1/')
    if [[ -z "$name" ]]; then
        return 1
    fi
    local url
    url=$(echo "$json" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for a in data.get('assets', []):
    if a['name'] == '$name':
        print(a['browser_download_url'])
        break
" 2>/dev/null || echo "$json" | grep -oE "\"browser_download_url\": *\"[^\"]+${name}\"" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    echo "$url"
}

# ── Main ─────────────────────────────────────────────────────────────
main() {
    require_cmd curl
    require_cmd tar

    local target
    target=$(detect_target)
    info "Detected platform: ${target}"

    # Decide which release tag to use.
    local tag=""
    if [[ -n "$VERSION_TAG" ]]; then
        tag="$VERSION_TAG"
    elif $USE_PRE; then
        tag="latest"
    fi

    info "Fetching release info..."
    local release_json
    if ! release_json=$(fetch_release "$tag" 2>/dev/null); then
        if [[ -z "$tag" ]]; then
            # No stable release yet — fall back to the "latest" pre-release.
            warn "No stable release found, trying the 'latest' pre-release..."
            tag="latest"
            release_json=$(fetch_release "$tag") || error "Could not fetch release info. Is the repo public?"
        else
            error "Could not fetch release info for tag '${tag}'."
        fi
    fi

    # Extract tag name (e.g. "v0.1.0" or "latest").
    local release_tag
    release_tag=$(echo "$release_json" | grep -oE '"tag_name": *"[^"]+"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    info "Release: ${release_tag}"

    # Find matching asset.
    local asset_url
    asset_url=$(find_asset_url "$release_json" "$target") || \
        error "No asset found for target '${target}' in release ${release_tag}. Available assets:"$'\n'"$(echo "$release_json" | grep -oE '"name": *"[^"]+\.(tar\.gz|zip)"' | sed -E 's/.*"([^"]+)".*/  - \1/')"

    # Download.
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "${tmp_dir}"' EXIT

    local archive="${tmp_dir}/archive.tar.gz"
    info "Downloading $(basename "${asset_url}")..."
    curl -fSL --progress-bar -o "${archive}" "${asset_url}"

    # Extract.
    info "Extracting..."
    tar -xzf "${archive}" -C "${tmp_dir}"

    # Find the extracted binary.
    local bin_src
    bin_src=$(find "${tmp_dir}" -type f -name "${BINARY}" -not -path "*/target/*" | head -1)
    [[ -n "$bin_src" ]] || error "Could not find ${BINARY} in the archive."

    # Install.
    mkdir -p "${INSTALL_DIR}"
    local dest="${INSTALL_DIR}/${BINARY}"

    # If we're upgrading a running binary (unlikely here, but safe).
    if [[ -f "$dest" ]]; then
        local old_ver
        old_ver=$("$dest" --version 2>/dev/null || echo "unknown")
        info "Upgrading ${old_ver} → ${release_tag}"
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
