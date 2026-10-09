#!/usr/bin/env bash
# Install the VectorCraft dev launcher for the current user: "VectorCraft (dev)" on the desktop and
# in the applications menu, with the app icon marked by a red "dev" bar. Opening it runs launch.sh
# in a terminal, which rebuilds the app when the code changed and starts it. This script builds
# (or downloads) the app first, writes the launchers, then starts the app.
# No root needed; run it again to update, uninstall.sh to remove.
#
# The app it launches (see common.sh):
#   - with Rust installed, this checkout's release build (target/release/vectorcraft), rebuilt
#     first when the sources changed;
#   - else this checkout's release build, when there is one;
#   - otherwise the AppImage from the latest GitHub Release, downloaded and sha256-checked into
#     ~/.local/share/vectorcraft/.
#
# Usage: dev-launcher/linux/install.sh [--local | --download] [--build | --no-build]
#                                   [--no-desktop-icon] [--no-launch] [--repo OWNER/NAME]
#   --local            use this checkout's build (fails if there is none and Rust isn't installed)
#   --download         use the latest release's AppImage, even with a local build or Rust
#   --build            build the release binary (the default with Rust; fails without it)
#   --no-build         use the local build as it is, even when the sources are newer
#   --no-desktop-icon  only add the applications-menu entry
#   --no-launch        don't start the app when done
#   --repo OWNER/NAME  GitHub repository to download from (default: this checkout's origin,
#                      falling back to storytold/vectorcraft when it has no release)
# The launcher keeps the options that choose the app.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

DESKTOP_ICON=1
LAUNCH=1
while [ $# -gt 0 ]; do
  case "$1" in
    --no-desktop-icon) DESKTOP_ICON=0; shift; continue ;;
    --no-launch) LAUNCH=0; shift; continue ;;
    -h | --help) awk 'NR > 1 && !/^#/ { exit } NR > 1' "$0"; exit 0 ;;
  esac
  n=0; parse_app_option "$@" || n=$?
  [ "$n" -gt 0 ] || { echo "unknown argument: $1 (see --help)" >&2; exit 2; }
  shift "$n"
done
OPTIONS="$(app_options)" # before prepare_app settles BUILD=auto

prepare_app
echo "==> The launcher will run $APP"

DESKTOP="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"
DESKTOP_FILE="$DESKTOP/VectorCraft-dev.desktop"

# --- Launchers from before the dev badge -----------------------------------------------------
# Earlier versions of this script installed the launcher under the release app's own id, name and
# icon, which hid a packaged VectorCraft. Remove those (not a packaged entry: see OLD_EXEC).
for old in "$APPS_DIR/$APP_ID.desktop" "$DESKTOP/VectorCraft.desktop"; do
  if [ -f "$old" ] && grep -qE "$OLD_EXEC" "$old"; then
    rm -f "$old"
    find "$ICONS_DIR/hicolor" -name "$APP_ID.*" -path '*/apps/*' -delete 2>/dev/null || true
    echo "    removed the old launcher $old"
  fi
done

# --- The dev icon: the app icon with a red "dev" bar along the bottom -------------------------
# Drawn over the app icon's 512×512 tile (rounded, rx=112), in a deeper red than its field.
make_dev_icon() {
  local src="$ROOT/assets/app-icon/hicolor/scalable/apps/$APP_ID.svg"
  local dir="$ICONS_DIR/hicolor/scalable/apps"
  local svg
  [ -f "$src" ] || die "missing app icon $src"
  svg="$(<"$src")"
  mkdir -p "$dir"
  {
    printf '%s' "${svg%</svg>*}"
    cat <<'SVG'
<clipPath id="dev-tile"><rect width="512" height="512" rx="112"/></clipPath><g clip-path="url(#dev-tile)"><rect y="400" width="512" height="112" fill="#b3001b"/><rect y="400" width="512" height="8" fill="#efe9dc"/></g><text x="256" y="488" text-anchor="middle" font-family="DejaVu Sans, Liberation Sans, Arial, Helvetica, sans-serif" font-weight="bold" font-size="84" letter-spacing="2" fill="#ffffff">dev</text></svg>
SVG
  } > "$dir/$DEV_ID.svg"
  # Sized PNGs too where we can render them; icon themes fall back to the SVG otherwise.
  if command -v rsvg-convert >/dev/null; then
    for size in 16 24 32 48 64 128 256 512; do
      mkdir -p "$ICONS_DIR/hicolor/${size}x$size/apps"
      rsvg-convert -w "$size" -h "$size" "$dir/$DEV_ID.svg" \
        -o "$ICONS_DIR/hicolor/${size}x$size/apps/$DEV_ID.png"
    done
  fi
}
make_dev_icon

# --- Launchers -------------------------------------------------------------------------------
mkdir -p "$APPS_DIR"
# A desktop-entry string: backslashes escaped (and nothing else needs it in our values).
desktop_string() { printf '%s' "${1//\\/\\\\}"; }
# One argument of an Exec line: quoted, with the characters the spec reserves inside quotes
# escaped, and % doubled so it isn't read as a field code.
exec_arg() {
  local a="$1"
  a="${a//\\/\\\\}"; a="${a//\"/\\\"}"; a="${a//\`/\\\`}"; a="${a//\$/\\\$}"; a="${a//%/%%}"
  desktop_string "\"$a\""
}
LAUNCH_SH="$HERE/launch.sh"
EXEC="$(exec_arg "$LAUNCH_SH")"
for opt in $OPTIONS; do EXEC+=" $(exec_arg "$opt")"; done
EXEC+=" -- %F"
ENTRY="$APPS_DIR/$DEV_ID.desktop"
# The packaged entry, renamed and pointed at launch.sh, which runs in a terminal so a build can be
# followed. awk reads the values from the environment, which keeps them verbatim.
D_EXEC="$EXEC" D_TRYEXEC="$(desktop_string "$LAUNCH_SH")" D_ICON="$DEV_ID" awk -F= '
  BEGIN {
    v["Name"] = "VectorCraft (dev)"
    v["Comment"] = "Your own build of VectorCraft, rebuilt when the code changed"
    v["Exec"] = ENVIRON["D_EXEC"]; v["TryExec"] = ENVIRON["D_TRYEXEC"]; v["Icon"] = ENVIRON["D_ICON"]
    v["Terminal"] = "true"; v["StartupNotify"] = "false"
  }
  $1 in v { print $1 "=" v[$1]; next }
  { print }' "$ROOT/packaging/linux/$APP_ID.desktop" > "$ENTRY"
chmod +x "$ENTRY"
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$ENTRY"; fi
echo "    menu:    $ENTRY"

if [ "$DESKTOP_ICON" = 1 ] && [ -d "$DESKTOP" ]; then
  cp -f "$ENTRY" "$DESKTOP_FILE"
  chmod +x "$DESKTOP_FILE"
  # Nemo / Nautilus / Caja run a desktop launcher without asking only once it is trusted.
  gio set "$DESKTOP_FILE" metadata::trusted true 2>/dev/null || true
  echo "    desktop: $DESKTOP_FILE"
fi

update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
gtk-update-icon-cache -f -t "$ICONS_DIR/hicolor" >/dev/null 2>&1 || true
echo "==> Done. Open \"VectorCraft (dev)\" from the desktop or the applications menu: it rebuilds"
echo "    the app when the code changed, then starts it."

if [ "$LAUNCH" = 1 ]; then start_app; fi
