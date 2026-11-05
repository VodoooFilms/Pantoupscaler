#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="$ROOT/src-tauri/resources/ffmpeg"
DEST="$DEST_DIR/ffmpeg"
EXECUTABLE="ffmpeg"
if [[ "$(uname -s)" == "MINGW"* || "$(uname -s)" == "CYGWIN"* || "$(uname -s)" == "MSYS"* ]]; then
  EXECUTABLE="ffmpeg.exe"
  DEST="$DEST_DIR/$EXECUTABLE"
fi

if [[ -x "$DEST" ]]; then
  echo "FFmpeg ya está preparado en $DEST"
  exit 0
fi

mkdir -p "$DEST_DIR"

CANDIDATES=(
  "$ROOT/build/ffmpeg/$EXECUTABLE"
  "/Applications/Pantoraya.app/Contents/Resources/ffmpeg/$EXECUTABLE"
  "$(command -v "$EXECUTABLE" 2>/dev/null || true)"
)

for candidate in "${CANDIDATES[@]}"; do
  if [[ -n "$candidate" && -f "$candidate" ]]; then
    cp "$candidate" "$DEST"
    chmod +x "$DEST"
    echo "FFmpeg copiado desde: $candidate"
    echo "Destino: $DEST"

    if command -v otool >/dev/null 2>&1; then
      if otool -L "$DEST" 2>/dev/null | tail -n +2 | grep -Eq '/(opt/homebrew|usr/local)/'; then
        echo "Aviso: este binario enlaza bibliotecas de Homebrew. Para distribuir la app usa un FFmpeg estático."
      fi
    fi
    exit 0
  fi
done

echo "No se encontró FFmpeg. Instálalo con 'brew install ffmpeg' (macOS) o coloca un binario estático en $DEST" >&2
exit 1
