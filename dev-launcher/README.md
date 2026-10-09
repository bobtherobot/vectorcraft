# Dev launcher

This sets up an icon on your desktop that runs the app with **your changes**, just like you run the
normal app.

As you work on the app, you need to build and run **your version**. If you also have the app
installed, it gets confusing: when you launch it, are you running your changes, or some other
version?

This creates a separate desktop launcher with a **dev** badge. Each time you open it, it:

1. rebuilds the app if the code changed (only what changed);
2. runs the latest version, with your changes.

## Setup (one time)

Run `install.sh` in a terminal. It builds the app, adds the launchers, and opens the app.

### Linux (Mint, Ubuntu, Debian, Fedora, …)

```sh
dev-launcher/linux/install.sh
```

Or double-click `dev-launcher/linux/install.sh` in your file manager and choose **Run in Terminal**
(Mint's Nemo asks how to open the script), so you can follow the first build or download.

After that, open **VectorCraft (dev)** from your desktop or applications menu.

## Things to know

1. Close the app before opening the launcher again: otherwise you get a second window (running the
   new build alongside the old one).
2. This only makes optimized builds of your code. It has nothing to do with publishing a release:
   the real installers (`.deb`, `.dmg`, `.msi`, …) are made by [`packaging/`](../packaging) in CI.
3. The launcher runs whatever is checked out, including uncommitted edits and the current branch.
4. No [Rust](https://rustup.rs), or too old a version to build the app? The launcher tells you, and
   runs the latest release instead.
5. The app's output goes to `~/.local/share/vectorcraft/dev-app.log`.
6. To remove the launcher, run `dev-launcher/linux/uninstall.sh`.
