# Contributing to TunnelPup

Thanks for helping. Bug reports, testing on systems the maintainer doesn't have, packaging for more platforms, docs and code are all welcome.

- **Found a bug?** Open an issue with the [bug report form](https://github.com/mayankjonwal02/Tunnelpup/issues/new?template=bug_report.yml). Please don't paste API tokens, and leave out domain names you'd rather keep private.
- **A question or an idea?** Start a thread in [Discussions](https://github.com/mayankjonwal02/Tunnelpup/discussions).
- **A security problem?** Report it privately, as described in [SECURITY.md](SECURITY.md), not in a public issue.
- **Looking for something to work on?** Issues labelled [`good first issue`](https://github.com/mayankjonwal02/Tunnelpup/labels/good%20first%20issue) are self-contained; [`help wanted`](https://github.com/mayankjonwal02/Tunnelpup/labels/help%20wanted) ones are bigger or need a system or skill the maintainer lacks. Comment on the issue before you start so two people don't do the same work.

## Set up

You need [Node.js](https://nodejs.org/) 20 or later (CI uses 22), [Rust](https://rustup.rs/) (stable) and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your system. On Ubuntu and Debian that's:

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libdbus-1-dev pkg-config
```

Then:

```sh
git clone https://github.com/mayankjonwal02/Tunnelpup
cd Tunnelpup
npm install          # also downloads cloudflared for your platform and checks its SHA-256
npm run tauri dev    # runs the app with hot reload for the interface
```

To try the app end to end you need a free Cloudflare account and a domain on it (a free DigitalPlat domain works). Use a test domain if you can: routes you add are public.

## Where things are

The README's [How the code is organised](README.md#build-from-source) table maps each file to what it does. In short: the interface is React in `src/` (`App.tsx`, plus the in-app guide in `Guide.tsx`), and everything that talks to Cloudflare, runs the connector or touches the system is Rust in `src-tauri/src/`, exposed to the interface through `commands.rs`.

## Checks

CI runs these on Linux and Windows for every pull request. Running them first saves a round trip:

```sh
npm run build                                   # type-checks and builds the interface

cd src-tauri
cargo fmt --check                               # formatting (see rustfmt.toml)
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Two tests talk to Cloudflare and DigitalPlat and are skipped by default. To run them you need a DigitalPlat domain that's active on Cloudflare:

```sh
TUNNELPUP_TEST_DOMAIN=yourname.dpdns.org cargo test -- --include-ignored
```

CI also checks the licence of every dependency (`npm run licenses`, which needs [cargo-about](https://github.com/EmbarkStudios/cargo-about)). If you add a dependency under a licence that `src-tauri/about.toml` doesn't accept, the pull request will fail; say why you need it and we'll sort it out.

## Pull requests

- Branch from `main` and keep each pull request to one change. Small ones get reviewed faster.
- Say what you changed, why, and how you tested it, including which system you tried it on.
- For interface changes, add a before and after screenshot.
- Keep the interface text plain and specific: say what happened and what to do next. No jargon where a plain word works, and no blaming the user.
- The `@tauri-apps/*` npm packages must stay on the same minor version as the matching Rust crates, or the build stops with a version mismatch.
- Never log, print or write the Cloudflare API token anywhere. It lives only in the system keychain and is only sent to Cloudflare.

Releases are made by the maintainer; [docs/RELEASING.md](docs/RELEASING.md) explains how.

## Licence

TunnelPup is released under the [MIT License](LICENSE). By contributing, you agree that your contribution is released under the same licence.
