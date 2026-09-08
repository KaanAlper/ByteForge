#!/usr/bin/env bash
# ByteForge — Linux tek satır kurulum.
#   curl -fsSL https://raw.githubusercontent.com/KaanAlper/ByteForge/master/install.sh | bash
#
# En son GitHub Release'ten taşınabilir .AppImage'ı indirir ve ~/.local/bin'e
# kurar. sudo gerektirmez. Kurulum dizinini BYTEFORGE_BIN ile değiştirebilirsin.
set -euo pipefail

REPO="KaanAlper/ByteForge"
BIN_DIR="${BYTEFORGE_BIN:-$HOME/.local/bin}"
DEST="$BIN_DIR/byteforge"
API="https://api.github.com/repos/$REPO/releases/latest"

say() { printf '\033[1;36m[ByteForge]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[ByteForge] hata:\033[0m %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl gerekli."

case "$(uname -m)" in
  x86_64|amd64) ARCH="amd64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) die "desteklenmeyen mimari: $(uname -m)" ;;
esac

say "En son sürüm aranıyor…"
JSON="$(curl -fsSL "$API")" || die "GitHub API'ye ulaşılamadı: $API"
TAG="$(printf '%s' "$JSON" | grep -oE '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4)"

# Mimariye uyan AppImage'ı seç; yoksa herhangi bir AppImage.
URL="$(printf '%s' "$JSON" | grep -oE '"browser_download_url": *"[^"]*\.AppImage"' \
      | cut -d'"' -f4 | grep -i "$ARCH" | head -1 || true)"
[ -n "$URL" ] || URL="$(printf '%s' "$JSON" | grep -oE '"browser_download_url": *"[^"]*\.AppImage"' \
      | cut -d'"' -f4 | head -1 || true)"
[ -n "$URL" ] || die "Bu sürümde AppImage bulunamadı — https://github.com/$REPO/releases"

say "ByteForge $TAG indiriliyor ($ARCH)…"
mkdir -p "$BIN_DIR"
curl -fL --progress-bar "$URL" -o "$DEST.tmp"
mv "$DEST.tmp" "$DEST"
chmod +x "$DEST"

say "Kuruldu: $DEST"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "Not: $BIN_DIR PATH'te değil. Kabuk profiline ekle:"
     printf '    export PATH="%s:$PATH"\n' "$BIN_DIR" ;;
esac
say "Çalıştır:  byteforge"
say "AppImage FUSE ister; yoksa:  byteforge --appimage-extract-and-run"
