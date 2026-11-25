#!/usr/bin/env bash
set -euo pipefail

REPO="KaanAlper/ByteForge"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
TARGET_BIN="${BYTEFORGE_BIN:-$HOME/.local/bin/byteforge}"

# Mimari kontrolü
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)
        ASSET_PATTERN="_amd64\\.AppImage$"
        ;;
    *)
        echo "❌ Desteklenmeyen mimari: $ARCH (şu an yalnızca x86_64/amd64 destekleniyor)." >&2
        exit 1
        ;;
esac

if ! command -v curl >/dev/null 2>&1; then
    echo "❌ 'curl' bulunamadı. Lütfen curl kurup tekrar deneyin." >&2
    exit 1
fi

echo "🔍 ByteForge son sürüm bilgisi alınıyor..."
RELEASE_JSON="$(curl -fsSL -H "Accept: application/vnd.github+json" "$API_URL")"

TAG="$(printf '%s\n' "$RELEASE_JSON" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
DOWNLOAD_URL="$(printf '%s\n' "$RELEASE_JSON" | sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | grep -E "$ASSET_PATTERN" | head -n1)"

if [ -z "$DOWNLOAD_URL" ]; then
    echo "❌ Son sürümde ($TAG) Linux AppImage paketi bulunamadı." >&2
    exit 1
fi

TARGET_DIR="$(dirname "$TARGET_BIN")"
mkdir -p "$TARGET_DIR"

TMP_FILE="$(mktemp "${TARGET_DIR}/.byteforge.XXXXXX")"
trap 'rm -f "$TMP_FILE"' EXIT

echo "⬇️  ByteForge ${TAG} indiriliyor..."
curl -fSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_FILE"
chmod +x "$TMP_FILE"
mv -f "$TMP_FILE" "$TARGET_BIN"

echo "✅ ByteForge ${TAG} kuruldu: $TARGET_BIN"

case ":${PATH}:" in
    *":${TARGET_DIR}:"*)
        echo "🚀 Başlatmak için: $(basename "$TARGET_BIN")"
        ;;
    *)
        echo "⚠️  '$TARGET_DIR' PATH içinde değil. Çalıştırmak için:"
        echo "    export PATH=\"$TARGET_DIR:\$PATH\""
        echo "    $(basename "$TARGET_BIN")"
        ;;
esac
