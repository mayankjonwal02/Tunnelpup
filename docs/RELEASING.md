# Releasing PortHop

Installers are built by GitHub Actions ([`.github/workflows/release.yml`](../.github/workflows/release.yml)), one job per system, and attached to a **draft** release:

| Job | Installers |
| --- | --- |
| Windows | `PortHop_<version>_x64-setup.exe`, `PortHop_<version>_x64_en-US.msi` |
| macOS (Apple Silicon) | `PortHop_<version>_aarch64.dmg` |
| macOS (Intel) | `PortHop_<version>_x64.dmg` |
| Linux | `PortHop_<version>_amd64.AppImage`, `.deb`, `.rpm` |

## Making a release

1. Set the new version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
2. Commit, then either push a tag (`git tag v0.2.0 && git push origin v0.2.0`) or run **Actions → Release → Run workflow**.
3. When all jobs are green, open **Releases**, check the draft, and publish it.

To try installers locally, `npm run tauri build` builds the ones for your own system into `src-tauri/target/release/bundle/`.

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

     `porthop-exe`
     ```xml
     <?xml version="1.0" encoding="utf-8"?>
     <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
       <zip-file>
         <pe-file path="porthop.exe" product-name="PortHop">
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
         <pe-file path="PortHop_*_x64-setup.exe">
           <authenticode-sign/>
         </pe-file>
         <msi-file path="PortHop_*_x64_en-US.msi">
           <authenticode-sign/>
         </msi-file>
       </zip-file>
     </artifact-configuration>
     ```
5. In this repository's **Settings → Secrets and variables → Actions**, add:
   - secret `SIGNPATH_API_TOKEN` (a SignPath user with submitter rights),
   - variables `SIGNPATH_ORGANIZATION_ID` and `SIGNPATH_PROJECT_SLUG` (and `SIGNPATH_SIGNING_POLICY_SLUG` if it isn't `release-signing`).

From the next release on, the Windows job builds `porthop.exe`, has SignPath sign it, packages the installers around the signed exe, has SignPath sign the installers, checks the signatures and attaches them. Each release waits (up to an hour) for you to approve the two signing requests in SignPath.

`cloudflared.exe` is never re-signed: it's Cloudflare's binary and already carries Cloudflare's signature.
