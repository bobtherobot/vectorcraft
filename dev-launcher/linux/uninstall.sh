#!/usr/bin/env bash
# Remove what install.sh added: the desktop and menu launchers, the icons and a downloaded
# AppImage. Leaves this checkout's build and your preferences and documents alone.
set -euo pipefail

APP_ID=ai.storyteller.vectorcraft
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"

rm -f "$DESKTOP/VectorCraft.desktop" "$DATA_HOME/applications/$APP_ID.desktop"
find "$DATA_HOME/icons/hicolor" -name "$APP_ID.*" -path '*/apps/*' -delete 2>/dev/null || true
rm -rf "$DATA_HOME/vectorcraft"

update-desktop-database "$DATA_HOME/applications" >/dev/null 2>&1 || true
gtk-update-icon-cache -f -t "$DATA_HOME/icons/hicolor" >/dev/null 2>&1 || true
echo "VectorCraft launchers removed."
