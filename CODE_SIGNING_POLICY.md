# Code signing policy

Windows builds aren't code-signed yet. Free signing through [SignPath Foundation](https://signpath.org/)'s program for open-source projects is being set up; until then Windows shows a SmartScreen warning the first time you run the installer (More info → Run anyway).

## What gets signed

Windows releases of TunnelPup are built from this repository by GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)). Once signing is set up, these are signed through SignPath:

- `tunnelpup.exe`, the TunnelPup application
- the installers: `TunnelPup_<version>_x64-setup.exe` and `TunnelPup_<version>_x64_en-US.msi`

TunnelPup also ships Cloudflare's tunnel connector, cloudflared, as `tunnelpup-cloudflared.exe`; the file is renamed so it can't clash with a copy of cloudflared you installed yourself. It is not built or signed by this project: it carries Cloudflare's own signature (Cloudflare, Inc.) and is otherwise unmodified.

## Team roles

- Committers and reviewers: [Mayank Jonwal](https://github.com/mayankjonwal02)
- Approvers: [Mayank Jonwal](https://github.com/mayankjonwal02)

Every release signing request will be approved manually by an approver.

## Privacy policy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

To do what you ask it to, TunnelPup connects to:

- **Cloudflare** (the API, with the token you provide, and Cloudflare's network through the tunnel connector) to manage your tunnel, DNS records and domains, and to serve the routes you create. See [Cloudflare's privacy policy](https://www.cloudflare.com/privacypolicy/).
- **Public DNS resolvers**, Cloudflare (1.1.1.1) and Google Public DNS, to check that your domains and routes are reachable. See [Google Public DNS privacy](https://developers.google.com/speed/public-dns/privacy).
- **DigitalPlat's nameservers**, only when you check whether a free domain name is available. See [DigitalPlat FreeDomain](https://domain.digitalplat.org/).

TunnelPup has no analytics or telemetry. Your Cloudflare API token is stored in your system's keychain and is only ever sent to Cloudflare.
