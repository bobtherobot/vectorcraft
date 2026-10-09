# Shared by install.sh and launch.sh (sourced, not run): picks the app to launch, building or
# downloading it first, and starts it.
#
# Which app:
#   - with Rust installed (at least the workspace's rust-version), this checkout's release build
#     (target/release/vectorcraft), brought up to date first: cargo rebuilds what changed since
#     the last build, and does nothing when nothing did (no signing warnings on any OS);
#   - otherwise (no Rust, or too old a Rust, which is said) the AppImage from the latest GitHub
#     Release, downloaded and sha256-checked into ~/.local/share/vectorcraft/ (again only when a
#     newer release is out). --local or --no-build run the local build as it is instead.

APP_ID=ai.storyteller.vectorcraft
DEV_ID=$APP_ID.dev
UPSTREAM_REPO=storytold/vectorcraft
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_HOME/vectorcraft"
APPIMAGE="$APP_DIR/vectorcraft.AppImage"
APPIMAGE_NAME="$APP_DIR/vectorcraft.AppImage.name" # the release file name, which holds the version
APPS_DIR="$DATA_HOME/applications"
ICONS_DIR="$DATA_HOME/icons"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
[[ "$TARGET_DIR" == /* ]] || TARGET_DIR="$ROOT/$TARGET_DIR"
LOCAL_BIN="$TARGET_DIR/release/vectorcraft"
# How to recognize a launcher from before the dev badge, under the release app's own id: it ran a
# local build or a downloaded AppImage, where the packaged entry runs `vectorcraft`.
OLD_EXEC='^(Exec|TryExec)=.*(/target/release/vectorcraft|/vectorcraft\.AppImage)'

MODE=auto
BUILD=auto
REPO=""
APP=""
ENVS=()

die() { echo "error: $*" >&2; exit 1; }

# Takes one of the options that choose the app (see install.sh --help); returns how many
# arguments it used, 0 when "$1" isn't one of them.
parse_app_option() {
  case "$1" in
    --local) MODE=local; return 1 ;;
    --download) MODE=download; return 1 ;;
    --build) BUILD=1; return 1 ;;
    --no-build) BUILD=0; return 1 ;;
    --repo) [ -n "${2:-}" ] || die "--repo needs OWNER/NAME"; REPO="$2"; return 2 ;;
  esac
  return 0
}

# The same choice as options, for the launcher to pass on to launch.sh.
app_options() {
  local opts=()
  [ "$MODE" = auto ] || opts+=("--$MODE")
  case "$BUILD" in 1) opts+=(--build) ;; 0) opts+=(--no-build) ;; esac
  [ -z "$REPO" ] || opts+=(--repo "$REPO")
  echo "${opts[*]}"
}

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

# Makes $APPIMAGE the latest release's AppImage, downloading it only when it is missing or a newer
# release is out. Offline, an AppImage downloaded earlier is used as it is.
download_appimage() {
  local arch pattern url="" sums name tmp
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
  echo "==> Checking for the latest release" >&2
  for r in "${repos[@]}"; do
    url="$(release_asset "$r" "$pattern")"
    if [ -n "$url" ]; then REPO="$r"; break; fi
    echo "    no Linux $arch AppImage in $r's latest release" >&2
  done
  if [ -z "$url" ]; then
    [ -x "$APPIMAGE" ] || die "no release to download; build it instead: $0 --local --build"
    echo "    couldn't reach the releases; using the AppImage downloaded earlier" >&2
    return 0
  fi

  name="${url##*/}"
  if [ -x "$APPIMAGE" ] && [ "$(cat "$APPIMAGE_NAME" 2>/dev/null)" = "$name" ]; then
    echo "    $name is up to date" >&2
    return 0
  fi
  echo "==> Downloading $name from $REPO" >&2
  mkdir -p "$APP_DIR"
  tmp="$APP_DIR/.download.AppImage"
  fetch "$url" "$tmp"
  sums="$(release_asset "$REPO" '/SHA256SUMS\.txt$')"
  if [ -n "$sums" ] && command -v sha256sum >/dev/null; then
    local expected actual
    expected="$(fetch "$sums" | awk -v n="$name" '$2 == n || $2 == "*" n { print $1 }')"
    actual="$(sha256sum "$tmp" | awk '{ print $1 }')"
    [ -n "$expected" ] || { rm -f "$tmp"; die "$name is not listed in the release's SHA256SUMS.txt"; }
    [ "$expected" = "$actual" ] || { rm -f "$tmp"; die "checksum mismatch for $name (download corrupted?)"; }
    echo "    sha256 verified" >&2
  else
    echo "    warning: no SHA256SUMS.txt in the release; checksum not verified" >&2
  fi
  chmod +x "$tmp"
  /bin/mv -f "$tmp" "$APPIMAGE"
  echo "$name" > "$APPIMAGE_NAME"
}

# The installed Rust version (e.g. 1.98.1), and the oldest one the workspace builds with
# (`rust-version` in Cargo.toml, e.g. 1.95).
rust_version() { cargo --version 2>/dev/null | awk '{ print $2 }'; }
rust_needed() { sed -nE 's/^rust-version *= *"([0-9.]+)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1; }
rust_is_new_enough() {
  local have need
  have="$(rust_version)"; need="$(rust_needed)"
  [ -n "$need" ] || return 0 # no minimum to check against
  [ -n "$have" ] || return 1
  [ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -n 1)" = "$need" ]
}

# Sets APP (and ENVS) to the app to launch, building or downloading it first.
prepare_app() {
  # With Rust installed, a local build is brought up to date rather than launched stale (or
  # downloaded). A release download needs no build. A launcher started from the desktop may not
  # have ~/.cargo/bin on its PATH (the shell profile adds it), so look there too.
  command -v cargo >/dev/null || [ ! -f "$HOME/.cargo/env" ] || . "$HOME/.cargo/env"
  local rust_problem=""
  if ! command -v cargo >/dev/null; then
    rust_problem="Rust isn't installed (https://rustup.rs)"
  elif ! rust_is_new_enough; then
    rust_problem="your Rust is $(rust_version), but VectorCraft needs $(rust_needed) or newer (update it with \`rustup update\`)"
  fi
  if [ "$BUILD" = auto ]; then
    BUILD=0
    if [ "$MODE" != download ]; then
      if [ -z "$rust_problem" ]; then
        BUILD=1
      elif [ "$MODE" = auto ]; then
        # A local build can't be brought up to date, so it may be stale: the release is current.
        echo "==> Can't build: $rust_problem."
        echo "    Launching the latest release instead."
        MODE=download
      else
        echo "==> Can't build: $rust_problem."
        echo "    Launching the existing build as it is."
      fi
    fi
  fi
  if [ "$BUILD" = 1 ]; then
    [ -z "$rust_problem" ] || die "can't build: $rust_problem"
    if [ -x "$LOCAL_BIN" ]; then
      echo "==> Updating the release build (only what changed is rebuilt)"
    else
      echo "==> Building VectorCraft (release; the first build takes a while)"
    fi
    (cd "$ROOT" && cargo build --release -p vectorcraft)
    [ -x "$LOCAL_BIN" ] || die "cargo didn't build $LOCAL_BIN: if your cargo config sets build.target-dir, set CARGO_TARGET_DIR to it instead"
    echo "    built $(date -r "$LOCAL_BIN" '+%Y-%m-%d %H:%M')"
  fi
  case "$MODE" in
    local) [ -x "$LOCAL_BIN" ] || die "no local build at $LOCAL_BIN (add --build)"; APP="$LOCAL_BIN" ;;
    download) download_appimage; APP="$APPIMAGE" ;;
    auto) if [ -x "$LOCAL_BIN" ]; then APP="$LOCAL_BIN"; else download_appimage; APP="$APPIMAGE"; fi ;;
  esac

  # An AppImage normally mounts itself with FUSE; without libfuse2 it can unpack itself instead.
  ENVS=()
  if [ "$APP" = "$APPIMAGE" ] && ! "$APP" --version >/dev/null 2>&1; then
    APPIMAGE_EXTRACT_AND_RUN=1 "$APP" --version >/dev/null 2>&1 \
      || die "the downloaded AppImage doesn't start ($APP --version); try building: $0 --local --build"
    ENVS+=("APPIMAGE_EXTRACT_AND_RUN=1")
  fi
}

# Starts APP with FILE… in its own session, so it outlives the terminal that launched it, and fails
# when the app quits within STARTUP_CHECK seconds (a crash at startup), showing the end of its log.
# Its output goes to ~/.local/share/vectorcraft/dev-app.log.
STARTUP_CHECK=1
start_app() { # start_app [FILE…]
  local log="$APP_DIR/dev-app.log" pid
  mkdir -p "$APP_DIR"
  echo "==> Starting $APP"
  # Not a process group leader, setsid execs the app in place, so $! is the app.
  env "${ENVS[@]}" setsid "$APP" "$@" >"$log" 2>&1 </dev/null &
  pid=$!
  sleep "$STARTUP_CHECK"
  if ! kill -0 "$pid" 2>/dev/null; then
    tail -n 20 "$log" >&2
    die "VectorCraft quit right after starting (log: $log)"
  fi
}
