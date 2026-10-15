# Third-party notices

## cloudflared

TunnelPup's installers include **cloudflared** 2026.10.0, Cloudflare's tunnel connector:

- Source: https://github.com/cloudflare/cloudflared
- Copyright: Cloudflare, Inc.
- Licence: Apache License, Version 2.0

It is redistributed unmodified, apart from the file name: it's installed as `tunnelpup-cloudflared` so it can't clash with a copy of cloudflared you installed yourself. On Windows it keeps Cloudflare's own code signature.

The full licence text is in [`src/licenses/cloudflared-LICENSE.txt`](src/licenses/cloudflared-LICENSE.txt), and TunnelPup shows it under **Settings → Open-source licenses**.

## Other dependencies

TunnelPup is built with [Tauri](https://tauri.app/) (MIT / Apache-2.0), [React](https://react.dev/) (MIT) and other open-source Rust crates and npm packages, each under its own licence. See `src-tauri/Cargo.lock` and `package-lock.json` for the full list.
