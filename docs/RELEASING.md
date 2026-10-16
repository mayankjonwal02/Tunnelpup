# Releasing TunnelPup

Installers are built by GitHub Actions ([`.github/workflows/release.yml`](../.github/workflows/release.yml)), one job per system, and attached to a **draft** release:

| Job | Installers |
| --- | --- |
| Windows | `TunnelPup_<version>_x64-setup.exe`, `TunnelPup_<version>_x64_en-US.msi` |
| macOS (Apple Silicon) | `TunnelPup_<version>_aarch64.dmg` |
| macOS (Intel) | `TunnelPup_<version>_x64.dmg` |
| Linux | `TunnelPup_<version>_amd64.AppImage`, `.deb`, `.rpm` |

## Making a release

1. Set the new version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
2. Merge that into `main` through a pull request, then run **Actions → Release → Run workflow**.
3. When all jobs are green, open **Releases**, check the draft, and publish it. Publishing creates the version tag (`v0.2.0`); don't push one yourself. Each run starts the draft over, with the notes from `.github/release-notes.md`, so make any edits to the draft after the last run.

To try installers locally, `npm run tauri build` builds the ones for your own system into `src-tauri/target/release/bundle/`.

Each build job also gathers the licence notices of every Rust crate and npm package the app is built from (`npm run licenses`, using [cargo-about](https://github.com/EmbarkStudios/cargo-about)), which the app shows under **Settings → Open-source licenses**. A dependency under a licence that `src-tauri/about.toml` doesn't accept stops the build, and CI flags it on the pull request that adds it.

To ship a newer cloudflared, change `CLOUDFLARED_VERSION` in `scripts/fetch-cloudflared.mjs` and `src-tauri/src/lib.rs`, and update the `SHA256` values in the script: GitHub shows each file's SHA-256 on the cloudflared release page. Check them against the checksums in Cloudflare's release notes, which for the macOS `.tgz` files are for the binary inside. A download that doesn't match is never used.

## Updates

The app's **Check for updates** reads `latest.json` from the latest release and installs an update only if it's signed with TunnelPup's update key. Release builds make the signed update files and `latest.json` through `src-tauri/tauri.release.conf.json`; local builds don't, so they don't need the key.

The key pair is made once, with `npx tauri signer generate -w "$env:USERPROFILE\.tauri\tunnelpup.key"`:

- the public key is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`;
- the private key and its password are the repository secrets `TAURI_SIGNING_PRIVATE_KEY` (the whole `.key` file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Keep a backup of the private key and its password. Without them no update can ever be signed again, and everyone would have to reinstall by hand.

## Code signing

- **macOS:** builds are ad-hoc signed, so users open the app with right-click → **Open** the first time. Removing that step needs the Apple Developer Program (notarization).
- **Linux:** packages aren't signed; no warnings are shown.
- **Windows:** signing through [SignPath Foundation](https://signpath.org/) (free for open-source projects) is built into the workflow but stays off until SignPath is set up. Until then Windows builds are unsigned and SmartScreen shows "Windows protected your PC" (→ More info → Run anyway).

### Turning on Windows signing (SignPath Foundation)

The repository already meets the requirements: an OSI licence ([MIT](../LICENSE)), the [code signing policy](../CODE_SIGNING_POLICY.md) linked from the README and release notes, and builds from source in GitHub Actions.

1. Publish at least one (unsigned) release; SignPath only signs software that has already been released.
2. Turn on two-factor authentication for GitHub (SignPath requires it for everyone on the team).
3. Apply at [signpath.org](https://signpath.org/apply).
4. Once approved, in SignPath:
   - link the project to the predefined **GitHub.com** trusted build system and install the SignPath GitHub App on this repository;
   - create a signing policy (the workflow assumes the slug `release-signing`);
   - create two artifact configurations with these slugs (adjust the version/metadata rules with SignPath if they ask):

     `tunnelpup-exe`
     ```xml
     <?xml version="1.0" encoding="utf-8"?>
     <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
       <zip-file>
         <pe-file path="tunnelpup.exe" product-name="TunnelPup">
           <authenticode-sign/>
         </pe-file>
       </zip-file>
     </artifact-configuration>
     ```

     `installers`
     ```xml
     <?xml version="1.0" encoding="utf-8"?>
     <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
       <zip-file>
         <pe-file path="TunnelPup_*_x64-setup.exe">
           <authenticode-sign/>
         </pe-file>
         <msi-file path="TunnelPup_*_x64_en-US.msi">
           <authenticode-sign/>
         </msi-file>
       </zip-file>
     </artifact-configuration>
     ```
5. In this repository's **Settings → Secrets and variables → Actions**, add:
   - secret `SIGNPATH_API_TOKEN` (a SignPath user with submitter rights),
   - variables `SIGNPATH_ORGANIZATION_ID` and `SIGNPATH_PROJECT_SLUG` (and `SIGNPATH_SIGNING_POLICY_SLUG` if it isn't `release-signing`).
6. Change the first line of [CODE_SIGNING_POLICY.md](../CODE_SIGNING_POLICY.md) to the wording SignPath asks for: "Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/)."
7. Make the signed Windows path produce update files too. It doesn't yet, so signed Windows installs wouldn't see updates. After SignPath signs the installers, sign each one for the updater with `npx tauri signer sign` (with the same two secrets), attach the `.sig` files, and add the Windows entries to the release's `latest.json`. Doing it earlier wouldn't work, because SignPath's signature changes the files.

From the next release on, the Windows job builds `tunnelpup.exe`, has SignPath sign it, packages the installers around the signed exe, has SignPath sign the installers, checks the signatures and attaches them. Each release waits (up to an hour) for you to approve the two signing requests in SignPath.

`tunnelpup-cloudflared.exe` is never re-signed: it's Cloudflare's cloudflared, only renamed, and already carries Cloudflare's signature.
