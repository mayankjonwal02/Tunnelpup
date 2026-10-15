# Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

## What gets signed

Windows releases of PortHop are built from this repository by GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)) and signed through SignPath:

- `porthop.exe`, the PortHop application
- the installers: `PortHop_<version>_x64-setup.exe` and `PortHop_<version>_x64_en-US.msi`

PortHop also ships Cloudflare's tunnel connector, cloudflared, as `porthop-cloudflared.exe`; the file is renamed so it can't clash with a copy of cloudflared you installed yourself. It is not built or signed by this project: it carries Cloudflare's own signature (Cloudflare, Inc.) and is otherwise unmodified.

## Team roles

- Committers and reviewers: [Mayank Jonwal](https://github.com/mayankjonwal02)
- Approvers: [Mayank Jonwal](https://github.com/mayankjonwal02)

Every release signing request is approved manually by an approver.

## Privacy policy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

To do what you ask it to, PortHop connects to:

- **Cloudflare** (the API, with the token you provide, and Cloudflare's network through the tunnel connector) to manage your tunnel, DNS records and domains, and to serve the routes you create. See [Cloudflare's privacy policy](https://www.cloudflare.com/privacypolicy/).
- **Public DNS resolvers**, Cloudflare (1.1.1.1) and Google Public DNS, to check that your domains and routes are reachable. See [Google Public DNS privacy](https://developers.google.com/speed/public-dns/privacy).
- **DigitalPlat's nameservers**, only when you check whether a free domain name is available. See [DigitalPlat FreeDomain](https://domain.digitalplat.org/).

PortHop has no analytics or telemetry. Your Cloudflare API token is stored in your system's keychain and is only ever sent to Cloudflare.
