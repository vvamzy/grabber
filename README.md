# Grabber

A native Windows download manager — IDM-style, written in Rust. No Electron, no webview: a raw Win32 GUI, a real Windows service, and a browser extension that hands downloads off to it.

![status](https://img.shields.io/badge/status-MVP-green)

## What's in the box

| Binary | Role |
|---|---|
| `dm-cli` | Download engine — multi-segment fetching over WinHTTP (raw FFI to the OS `winhttp.lib`, no bundled networking) |
| `dm-service` | Windows service (`DownloadManager`) exposing a loopback JSON API on `http://127.0.0.1:7790` |
| `dm-ui` | The GUI — rebar + toolbar, category tree, file list, download progress windows, statistics view. Win32 controls, ~16 MB resident |
| `dm-host` | Native messaging bridge between the browser and the service |
| `extension/` | MV3 Chrome/Edge extension — intercepts the browser's own download and cancels the save dialog before it appears |

Everything installs as a single `setup.exe` (Inno Setup): service, UI, native-host registration, and the extension folder.

## Install

Grab `setup.exe` from the [Releases](../../releases) page and run it (Windows x64, admin required).

Then in your browser:

1. `chrome://extensions` (or `edge://extensions`) → enable **Developer mode** → **Load unpacked** → pick the `extension` folder from the install directory.
2. Turn off "Ask where to save each file" in the browser's download settings — otherwise the browser's dialog wins the race and the extension never sees the download.

## Development

Prerequisites:

- Rust stable (`x86_64-pc-windows-msvc`)
- Visual Studio 2022 Build Tools with the C++ workload (for `link.exe`)
- Inno Setup 6 (only needed to compile the installer)

```powershell
cargo build --release
```

Binaries land in `target/release/`. To build the installer locally:

```powershell
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" setup.iss   # → dist\setup.exe
```

## Repository layout

```
crates/engine/    core download logic + stats (library, also builds dm-cli)
crates/service/   Windows service + HTTP API
crates/host/      native messaging host
crates/ui/        Win32 GUI
extension/        MV3 browser extension
setup.iss         Inno Setup installer script
VERSION           single source of truth for the release version
.github/workflows CI (dev, PRs) and release (main) pipelines
```

## Releases

The pipeline is version-driven and hands-off:

1. Work lands on `dev` — CI builds it, PRs must pass the `build` check.
2. Bump `VERSION`.
3. Open a PR `dev` → `main`. Merging triggers the release workflow, which builds all four crates, compiles `setup.exe` with that version, tags `vX.Y.Z`, and publishes a GitHub Release.

Promoting to `main` with a version that's already tagged fails the run on purpose — you can't re-release a version.

## License

MIT (see `Cargo.toml`).
