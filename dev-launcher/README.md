# Dev launcher

A **VectorCraft (dev)** icon on your desktop and in your applications menu that opens **your own
build**, with whatever changes you're working on. Its icon is the app icon with a red **dev** bar
along the bottom, so it can't be mistaken for an installed VectorCraft, which keeps its own entry.

```
edit code  ──►  double-click "VectorCraft (dev)"  ──►  rebuild if the code changed  ──►  app opens
```

- `install.sh` is a **one-time setup**: it builds the app, adds the launchers, and opens the app.
- Opening the launcher runs [`linux/launch.sh`](linux/launch.sh) in a terminal window: it brings
  `target/release/vectorcraft` (this checkout's optimized build) up to date, starts the app and
  closes. Cargo rebuilds only what changed, so with nothing new the terminal is gone within a
  couple of seconds; after a change you can follow the build there. If the build fails, the
  terminal stays open on the error until you press Enter.
- `cargo devapp` rebuilds the same binary from the command line (an alias for
  `cargo build --release -p vectorcraft`).

Close the app before opening the launcher again: otherwise you get a second window (running the new
build alongside the old one).

`--release` here only means an optimized build. It has nothing to do with publishing a release:
the real installers (`.deb`, `.dmg`, `.msi`, …) are made by [`packaging/`](../packaging) in CI.

Things to know:

- the launcher runs whatever is checked out, including uncommitted edits and the current branch;
- it runs the scripts in this checkout: move or delete the checkout and it stops working (run
  `install.sh` again from the new place);
- builds into another `CARGO_TARGET_DIR` (e.g. a parallel agent's) don't change it;
- the app's output goes to `~/.local/share/vectorcraft/dev-app.log`.

Without Rust, the same launcher can instead run the latest release's AppImage (below).

## Linux (Mint, Ubuntu, Debian, Fedora, …)

```sh
dev-launcher/linux/install.sh
```

Or double-click `dev-launcher/linux/install.sh` in your file manager and choose **Run in Terminal**
(Mint's Nemo asks how to open the script), so you can follow the first build or download.

It adds **VectorCraft (dev)** to your desktop and applications menu. It launches:

- with [Rust](https://rustup.rs) installed, this checkout's build (`target/release/vectorcraft`),
  rebuilt first when the code changed. Built on your own machine, it opens without the macOS and
  Windows warnings that unsigned downloads get;
- else this checkout's build, when there is one;
- otherwise the AppImage from the latest release, downloaded to `~/.local/share/vectorcraft/`
  and checked against the release's `SHA256SUMS.txt`. Each launch checks for a newer release and
  downloads only when there is one; offline, it opens the AppImage it has.

Options (`--help` lists them). The launcher keeps the ones that choose the app:

| Option | What it does |
| --- | --- |
| `--download` | use the release AppImage even if you have a local build or Rust |
| `--build` | build from source first (the default with [Rust](https://rustup.rs); fails without it) |
| `--no-build` | launch the local build as it is, even when the sources are newer |
| `--no-desktop-icon` | only add the applications-menu entry |
| `--no-launch` | don't open the app when the install is done |
| `--repo OWNER/NAME` | download from another GitHub repository |

Run `install.sh` again to change the options or refresh the launchers. It also removes the launcher
older versions of this script made under the plain **VectorCraft** name.
`dev-launcher/linux/uninstall.sh` removes the launchers, icons and any downloaded AppImage, and
leaves your documents and preferences alone.

No script needed? Download `vectorcraft-<version>-linux-x86_64.AppImage` from the releases page,
make it executable (right-click › Properties › Permissions, or `chmod +x`), and double-click it.
The release's `.deb` and `.rpm` install it system-wide with the menu entry.

## macOS and Windows

Not scripted yet: use the release's `.dmg` (macOS) or `.msi` / portable `.zip` (Windows).
