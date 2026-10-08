#!/usr/bin/env bash
# Install VectorCraft for the current user: a launcher on the desktop and in the applications
# menu, with the app icon. No root needed; run it again to update, uninstall.sh to remove.
#
# Which app it launches:
#   - with Rust installed, this checkout's release build (target/release/vectorcraft), brought up
#     to date first: cargo rebuilds what changed since the last build, and does nothing when
#     nothing did (no signing warnings on any OS);
#   - else this checkout's release build, when there is one;
#   - otherwise the AppImage from the latest GitHub Release, downloaded and sha256-checked into
#     ~/.local/share/vectorcraft/.
#
# Usage: dev-launcher/linux/install.sh [--local | --download] [--build | --no-build]
#                                   [--no-desktop-icon] [--repo OWNER/NAME]
#   --local            use this checkout's build (fails if there is none and Rust isn't installed)
#   --download         use the latest release's AppImage, even with a local build or Rust
#   --build            build the release binary (the default with Rust; fails without it)
#   --no-build         use the local build as it is, even when the sources are newer
#   --no-desktop-icon  only add the applications-menu entry
#   --repo OWNER/NAME  GitHub repository to download from (default: this checkout's origin,
#                      falling back to storytold/vectorcraft when it has no release)
set -euo pipefail

APP_ID=ai.storyteller.vectorcraft
UPSTREAM_REPO=storytold/vectorcraft
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_HOME/vectorcraft"
APPS_DIR="$DATA_HOME/applications"
ICONS_DIR="$DATA_HOME/icons"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
[[ "$TARGET_DIR" == /* ]] || TARGET_DIR="$ROOT/$TARGET_DIR"
LOCAL_BIN="$TARGET_DIR/release/vectorcraft"

MODE=auto
BUILD=auto
DESKTOP_ICON=1
REPO=""
while [ $# -gt 0 ]; do
  case "$1" in
    --local) MODE=local; shift ;;
    --download) MODE=download; shift ;;
    --build) BUILD=1; shift ;;
    --no-build) BUILD=0; shift ;;
    --no-desktop-icon) DESKTOP_ICON=0; shift ;;
    --repo) REPO="${2:-}"; shift 2 ;;
    -h | --help) awk 'NR > 1 && !/^#/ { exit } NR > 1' "$0"; exit 0 ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 2 ;;
  esac
done

die() { echo "error: $*" >&2; exit 1; }

# The GitHub repository this checkout was cloned from, e.g. bobtherobot/vectorcraft.
origin_repo() {
  local url
  url="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
  case "$url" in
    *github.com[:/]*) url="${url#*github.com[:/]}"; echo "${url%.git}" ;;
  esac
}

fetch() { # fetch URL [OUTPUT]
  if command -v curl >/dev/null; then
    if [ $# -gt 1 ]; then curl -fL --progress-bar -o "$2" "$1"; else curl -fsSL "$1"; fi
  elif command -v wget >/dev/null; then
    if [ $# -gt 1 ]; then wget -q --show-progress -O "$2" "$1"; else wget -qO- "$1"; fi
  else
    die "needs curl or wget to download the app"
  fi
}

# The download URL of the latest release's file named like PATTERN (an extended regex), or nothing.
release_asset() { # release_asset REPO PATTERN
  fetch "https://api.github.com/repos/$1/releases/latest" 2>/dev/null \
    | grep -oE '"browser_download_url": *"[^"]+"' \
    | sed -E 's/.*"(https[^"]+)"/\1/' \
    | grep -E "$2" | head -n 1 || true
}

# Prints only the AppImage's path on stdout; progress goes to stderr.
download_appimage() {
  local arch pattern url sums name tmp
  arch="$(uname -m)"
  case "$arch" in
    x86_64 | aarch64) ;;
    arm64) arch=aarch64 ;;
    *) die "no prebuilt VectorCraft for $arch; build it instead: $0 --local --build" ;;
  esac
  pattern="/vectorcraft-[^/]*-linux-$arch\\.AppImage$"
  local repos=()
  if [ -n "$REPO" ]; then repos=("$REPO"); else
    local o; o="$(origin_repo)"
    [ -n "$o" ] && repos+=("$o")
    [ "$o" != "$UPSTREAM_REPO" ] && repos+=("$UPSTREAM_REPO")
  fi
  for r in "${repos[@]}"; do
    url="$(release_asset "$r" "$pattern")"
    if [ -n "$url" ]; then REPO="$r"; break; fi
    echo "    no Linux $arch AppImage in $r's latest release" >&2
  done
  [ -n "$url" ] || die "no release to download; build it instead: $0 --local --build"

  name="${url##*/}"
  echo "==> Downloading $name from $REPO" >&2
  mkdir -p "$APP_DIR"
  tmp="$APP_DIR/.download.AppImage"
  fetch "$url" "$tmp"
  sums="$(release_asset "$REPO" '/SHA256SUMS\.txt$')"
  if [ -n "$sums" ] && command -v sha256sum >/dev/null; then
    local expected actual
    expected="$(fetch "$sums" | awk -v n="$name" '$2 == n || $2 == "*" n { print $1 }')"
    actual="$(sha256sum "$tmp" | awk '{ print $1 }')"
    [ -n "$expected" ] || die "$name is not listed in the release's SHA256SUMS.txt"
    [ "$expected" = "$actual" ] || { rm -f "$tmp"; die "checksum mismatch for $name (download corrupted?)"; }
    echo "    sha256 verified" >&2
  else
    echo "    warning: no SHA256SUMS.txt in the release; checksum not verified" >&2
  fi
  chmod +x "$tmp"
  /bin/mv -f "$tmp" "$APP_DIR/vectorcraft.AppImage"
  echo "$APP_DIR/vectorcraft.AppImage"
}

# --- Pick the app ---------------------------------------------------------------------------
# With Rust installed, a local build is brought up to date rather than launched stale (or
# downloaded). A release download needs no build.
if [ "$BUILD" = auto ]; then
  BUILD=0
  if [ "$MODE" != download ] && command -v cargo >/dev/null; then BUILD=1; fi
fi
if [ "$BUILD" = 1 ]; then
  command -v cargo >/dev/null || die "--build needs Rust (https://rustup.rs)"
  if [ -x "$LOCAL_BIN" ]; then
    echo "==> Updating the release build (only what changed is rebuilt; --no-build skips this)"
  else
    echo "==> Building VectorCraft (release; the first build takes a while)"
  fi
  (cd "$ROOT" && cargo build --release -p vectorcraft)
  echo "    built $(date -r "$LOCAL_BIN" '+%Y-%m-%d %H:%M')"
fi
case "$MODE" in
  local) [ -x "$LOCAL_BIN" ] || die "no local build at $LOCAL_BIN (add --build)"; APP="$LOCAL_BIN" ;;
  download) APP="$(download_appimage)" ;;
  auto) if [ -x "$LOCAL_BIN" ]; then APP="$LOCAL_BIN"; else APP="$(download_appimage)"; fi ;;
esac

# An AppImage normally mounts itself with FUSE; without libfuse2 it can unpack itself instead.
ENVS=()
if [[ "$APP" == *.AppImage ]] && ! "$APP" --version >/dev/null 2>&1; then
  APPIMAGE_EXTRACT_AND_RUN=1 "$APP" --version >/dev/null 2>&1 \
    || die "the downloaded AppImage doesn't start ($APP --version); try building: $0 --local --build"
  ENVS+=("APPIMAGE_EXTRACT_AND_RUN=1")
fi
echo "==> Launchers will run $APP"

# --- Icons and launchers --------------------------------------------------------------------
mkdir -p "$ICONS_DIR" "$APPS_DIR"
cp -Rf "$ROOT/assets/app-icon/hicolor" "$ICONS_DIR/"

EXEC="\"$APP\" %F"
[ ${#ENVS[@]} -gt 0 ] && EXEC="env ${ENVS[*]} $EXEC"
ENTRY="$APPS_DIR/$APP_ID.desktop"
# The packaged entry, pointed at this app. `|` can't appear in a path we write, so it delimits.
sed -e "s|^Exec=.*|Exec=$EXEC|" -e "s|^TryExec=.*|TryExec=$APP|" \
  "$ROOT/packaging/linux/$APP_ID.desktop" > "$ENTRY"
chmod +x "$ENTRY"
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$ENTRY"; fi
echo "    menu:    $ENTRY"

if [ "$DESKTOP_ICON" = 1 ]; then
  DESKTOP="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"
  if [ -d "$DESKTOP" ]; then
    cp -f "$ENTRY" "$DESKTOP/VectorCraft.desktop"
    chmod +x "$DESKTOP/VectorCraft.desktop"
    # Nemo / Nautilus / Caja run a desktop launcher without asking only once it is trusted.
    gio set "$DESKTOP/VectorCraft.desktop" metadata::trusted true 2>/dev/null || true
    echo "    desktop: $DESKTOP/VectorCraft.desktop"
  fi
fi

update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
gtk-update-icon-cache -f -t "$ICONS_DIR/hicolor" >/dev/null 2>&1 || true
echo "==> Done. Start VectorCraft from the desktop icon or the applications menu."
if [ "$APP" = "$LOCAL_BIN" ]; then
  echo "    After editing the code, run \`cargo devapp\` and reopen the app to see your changes."
fi
