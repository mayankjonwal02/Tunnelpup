# PortHop

Put an app running on your computer on your own domain: `localhost:3000` → `https://app.yourdomain.com`, through a Cloudflare Tunnel. No ports opened on your router, HTTPS included.

## Download

Get the installer for your system from the [latest release](https://github.com/mayankjonwal02/Porthop/releases/latest):

| System | File |
| --- | --- |
| Windows 10/11 | `PortHop_<version>_x64-setup.exe` (or the `.msi`) |
| macOS, Apple Silicon (M1 and later) | `PortHop_<version>_aarch64.dmg` |
| macOS, Intel | `PortHop_<version>_x64.dmg` |
| Linux, any distribution | `PortHop_<version>_amd64.AppImage` |
| Ubuntu / Debian | `PortHop_<version>_amd64.deb` |
| Fedora / openSUSE | `PortHop-<version>-1.x86_64.rpm` |

The first time you open it on macOS, right-click PortHop in Applications and choose **Open**, since the app isn't notarized by Apple.

## What you need

- A free [Cloudflare](https://dash.cloudflare.com/sign-up) account.
- A domain on Cloudflare. If you don't have one, PortHop can walk you through getting a free one from [DigitalPlat](https://domain.digitalplat.org/).

## What it does

- **Routes:** map `name.yourdomain.com` to any local port. PortHop sets up the tunnel routing and DNS, then checks the link actually loads before listing it.
- **Domains:** add a domain you own, or get a free `.dpdns.org` / `.us.kg` / `.qzz.io` / `.xx.kg` one, and follow it until its nameservers and HTTPS certificate are ready.
- **Status you can trust:** banners when the tunnel is stopped or reconnecting, "App not running" when a route's app is down, readable connector logs.
- **Runs in the background:** closes to the system tray and can start when you log in.
- **Guide:** a built-in guide covers setup and the common problems.

The Cloudflare API token you give PortHop is kept in your system's keychain, never in a file.

## Build from source

You need [Node.js](https://nodejs.org/) 20+, [Rust](https://rustup.rs/) and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```sh
npm install          # also downloads the cloudflared connector for your platform
npm run tauri dev    # run the app in development
npm run tauri build  # build installers into src-tauri/target/release/bundle/
```

Installers for every system are built by GitHub Actions; see [docs/RELEASING.md](docs/RELEASING.md).

## Code signing policy

Windows releases are being set up for free code signing through SignPath Foundation's program for open-source projects. See the [code signing policy](CODE_SIGNING_POLICY.md).

## License

[MIT](LICENSE). PortHop bundles [cloudflared](https://github.com/cloudflare/cloudflared) (Apache License 2.0) by Cloudflare, Inc.
