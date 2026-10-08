# Dev launcher

A VectorCraft icon on your desktop and in your applications menu that opens **your own build**,
with whatever changes you're working on.

```
edit code  ──►  cargo devapp  ──►  target/release/vectorcraft  ◄──  desktop icon
```

- `install.sh` is a **one-time setup**: it adds the icon and menu entry, pointed at this
  checkout's optimized build, `target/release/vectorcraft`.
- `cargo devapp` **applies your changes**: it rebuilds that binary (an alias for
  `cargo build --release -p vectorcraft`; cargo rebuilds only what changed). Close the app if it
  is running, then double-click the icon.

`--release` here only means an optimized build. It has nothing to do with publishing a release:
the real installers (`.deb`, `.dmg`, `.msi`, …) are made by [`packaging/`](../packaging) in CI.

Things to know:

- the icon runs whatever is checked out and built, including uncommitted edits and the
  current branch;
- builds into another `CARGO_TARGET_DIR` (e.g. a parallel agent's) don't change it;
- `cargo clean` leaves the icon with nothing to open until the next `cargo devapp`.

Without Rust, the same script can instead install the latest release's AppImage (below).

## Linux (Mint, Ubuntu, Debian, Fedora, …)

```sh
dev-launcher/linux/install.sh
```

Or double-click `dev-launcher/linux/install.sh` in your file manager and choose **Run in Terminal**
(Mint's Nemo asks how to open the script), so you can follow a build or download.

It adds **VectorCraft** to your desktop and applications menu, with its icon. It launches:

- with [Rust](https://rustup.rs) installed, this checkout's build (`target/release/vectorcraft`),
  brought up to date first: cargo rebuilds only what changed, and does nothing when nothing did.
  Built on your own machine, it opens without the macOS and Windows warnings that unsigned
  downloads get;
- else this checkout's build, when there is one;
- otherwise the AppImage from the latest release, downloaded to `~/.local/share/vectorcraft/`
  and checked against the release's `SHA256SUMS.txt`.

Options (`--help` lists them):

| Option | What it does |
| --- | --- |
| `--download` | use the release AppImage even if you have a local build or Rust |
| `--build` | build from source first (the default with [Rust](https://rustup.rs); fails without it) |
| `--no-build` | launch the local build as it is, even when the sources are newer |
| `--no-desktop-icon` | only add the applications-menu entry |
| `--repo OWNER/NAME` | download from another GitHub repository |

Run it again to rebuild and refresh the launchers (`cargo devapp` is enough to rebuild). `dev-launcher/linux/uninstall.sh` removes the launchers, icons and any
downloaded AppImage, and leaves your documents and preferences alone.

No script needed? Download `vectorcraft-<version>-linux-x86_64.AppImage` from the releases page,
make it executable (right-click › Properties › Permissions, or `chmod +x`), and double-click it.
The release's `.deb` and `.rpm` install it system-wide with the menu entry.

## macOS and Windows

Not scripted yet: use the release's `.dmg` (macOS) or `.msi` / portable `.zip` (Windows).
