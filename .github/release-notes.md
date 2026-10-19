Download the installer for your system:

| System | File |
| --- | --- |
| Windows 10/11 | `TunnelPup_{version}_x64-setup.exe` (or the `.msi`) |
| macOS, Apple Silicon (M1 and later) | `TunnelPup_{version}_aarch64.dmg` |
| macOS, Intel | `TunnelPup_{version}_x64.dmg` |
| Linux, any distribution | `TunnelPup_{version}_amd64.AppImage` (make it executable, then run it) |
| Ubuntu / Debian | `TunnelPup_{version}_amd64.deb` |
| Fedora / openSUSE | `TunnelPup-{version}-1.x86_64.rpm` |

**First launch:**
- **macOS:** the app isn't notarized by Apple, so right-click TunnelPup in Applications → **Open** → **Open**. Or System Settings → Privacy & Security → **Open Anyway**.
- **Windows:** if "Windows protected your PC" appears, choose **More info** → **Run anyway**.

**Checking a download:** `SHA256SUMS.txt` lists the SHA-256 of every file. In the folder with your download, run `sha256sum -c SHA256SUMS.txt --ignore-missing` (Linux), `shasum -a 256 -c SHA256SUMS.txt --ignore-missing` (macOS) or `Get-FileHash <file>` (Windows PowerShell) and compare.

[Code signing policy]({repo}/blob/main/CODE_SIGNING_POLICY.md)
