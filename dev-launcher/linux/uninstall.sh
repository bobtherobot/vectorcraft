#!/usr/bin/env bash
# Remove what install.sh added: the desktop and menu launchers, the icons and a downloaded
# AppImage. Leaves this checkout's build and your preferences and documents alone.
set -euo pipefail

APP_ID=ai.storyteller.vectorcraft
DEV_ID=$APP_ID.dev
OLD_EXEC='^(Exec|TryExec)=.*(/target/release/vectorcraft|/vectorcraft\.AppImage)'
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"

rm -f "$DESKTOP/VectorCraft-dev.desktop" "$DATA_HOME/applications/$DEV_ID.desktop"
find "$DATA_HOME/icons/hicolor" -name "$DEV_ID.*" -path '*/apps/*' -delete 2>/dev/null || true
# Launchers from before the dev badge used the release app's id, name and icon: remove those,
# recognized by running a local build or a downloaded AppImage (never a packaged entry).
for old in "$DATA_HOME/applications/$APP_ID.desktop" "$DESKTOP/VectorCraft.desktop"; do
  if [ -f "$old" ] && grep -qE "$OLD_EXEC" "$old"; then
    rm -f "$old"
    find "$DATA_HOME/icons/hicolor" -name "$APP_ID.*" -path '*/apps/*' -delete 2>/dev/null || true
  fi
done
rm -rf "$DATA_HOME/vectorcraft"

update-desktop-database "$DATA_HOME/applications" >/dev/null 2>&1 || true
gtk-update-icon-cache -f -t "$DATA_HOME/icons/hicolor" >/dev/null 2>&1 || true
echo "VectorCraft dev launchers removed."
