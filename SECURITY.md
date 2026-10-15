# Security policy

## Reporting a vulnerability

Please report security problems **privately**, through this repository's **Security → Report a vulnerability** form on GitHub. Don't open a public issue for them.

You'll get a reply within a few days. Fixes for serious problems ship as a new release as soon as they're ready.

## Supported versions

Only the latest release gets security fixes.

## What's in scope

- **Your Cloudflare API token:** it must only ever be stored in the system keychain and sent to Cloudflare, and must never appear in files, logs or a process's command line.
- **The tunnel connector:** how PortHop starts, supervises and stops it. Problems in cloudflared itself belong to Cloudflare: see https://www.cloudflare.com/disclosure/.
- **The release builds** published on this repository's Releases page.
