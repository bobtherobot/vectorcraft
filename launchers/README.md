# Launchers

Get a VectorCraft icon on your desktop and in your applications menu, with or without building
the app yourself.

The app itself isn't committed here (it's 50–75 MB per platform and would bloat every clone).
Ready-made builds are attached to each [GitHub Release](../../../releases); the scripts below
fetch the right one for you, or use your own build when you have one.

## Linux (Mint, Ubuntu, Debian, Fedora, …)

```sh
launchers/linux/install.sh
```

Or double-click `launchers/linux/install.sh` in your file manager and choose **Run in Terminal**
(Mint's Nemo asks how to open the script), so you can follow a build or download.

It adds **VectorCraft** to your desktop and applications menu, with its icon. It launches:

- this checkout's build (`target/release/vectorcraft`) when there is one, so after
  `cargo build --release -p vectorcraft` the icon always opens your latest build;
- else, when [Rust](https://rustup.rs) is installed, that build, made first. Built on your own
  machine, it opens without the macOS and Windows warnings that unsigned downloads get;
- otherwise the AppImage from the latest release, downloaded to `~/.local/share/vectorcraft/`
  and checked against the release's `SHA256SUMS.txt`.

Options (`--help` lists them):

| Option | What it does |
| --- | --- |
| `--download` | use the release AppImage even if you have a local build or Rust |
| `--local --build` | build from source first (needs [Rust](https://rustup.rs)) |
| `--no-desktop-icon` | only add the applications-menu entry |
| `--repo OWNER/NAME` | download from another GitHub repository |

Run it again to update. `launchers/linux/uninstall.sh` removes the launchers, icons and any
downloaded AppImage, and leaves your documents and preferences alone.

No script needed? Download `vectorcraft-<version>-linux-x86_64.AppImage` from the releases page,
make it executable (right-click › Properties › Permissions, or `chmod +x`), and double-click it.
The release's `.deb` and `.rpm` install it system-wide with the menu entry.

## macOS and Windows

Not scripted yet: use the release's `.dmg` (macOS) or `.msi` / portable `.zip` (Windows).
