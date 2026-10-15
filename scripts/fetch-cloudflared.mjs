#!/usr/bin/env node
// Downloads the official cloudflared binary into src-tauri/binaries/ with the
// file name Tauri's sidecar system expects: tunnelpup-cloudflared-<rust-target-triple>[.exe]
//
// It's renamed so it can't clash with a cloudflared the user installed themselves: Linux
// packages put bundled binaries in /usr/bin, where Cloudflare's package installs cloudflared.
//
// Usage:
//   node scripts/fetch-cloudflared.mjs            # host platform only
//   node scripts/fetch-cloudflared.mjs --all      # every supported target (for CI)
//   node scripts/fetch-cloudflared.mjs --target aarch64-apple-darwin
//
// Bump CLOUDFLARED_VERSION when you ship an app update with a newer connector.

import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const CLOUDFLARED_VERSION = "2026.10.0";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT_DIR = join(ROOT, "src-tauri", "binaries");
const BASE = `https://github.com/cloudflare/cloudflared/releases/download/${CLOUDFLARED_VERSION}`;

// Rust target triple -> release asset name
const ASSETS = {
  "x86_64-pc-windows-msvc": "cloudflared-windows-amd64.exe",
  // No native Windows arm64 build is published; the amd64 build runs under emulation.
  "aarch64-pc-windows-msvc": "cloudflared-windows-amd64.exe",
  "x86_64-apple-darwin": "cloudflared-darwin-amd64.tgz",
  "aarch64-apple-darwin": "cloudflared-darwin-arm64.tgz",
  "x86_64-unknown-linux-gnu": "cloudflared-linux-amd64",
  "aarch64-unknown-linux-gnu": "cloudflared-linux-arm64",
};

function hostTriple() {
  const out = execSync("rustc -vV", { encoding: "utf8" });
  const m = out.match(/^host:\s*(\S+)/m);
  if (!m) throw new Error("Could not determine host target triple from `rustc -vV`");
  return m[1];
}

// Two independent SHA-256 sources from the GitHub release, both best effort: if the API is
// unreachable or rate-limited we warn and continue unverified.
//  - digests: GitHub's own checksum of each release file, exactly as it's served.
//  - notes: the checksums Cloudflare lists in the release notes. For the plain binaries these
//    are the file itself; for the macOS .tgz archives they're the cloudflared binary inside.
async function fetchChecksums() {
  try {
    // In CI, GITHUB_TOKEN avoids the low anonymous rate limit shared by all runners.
    const auth = process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {};
    const res = await fetch(
      `https://api.github.com/repos/cloudflare/cloudflared/releases/tags/${CLOUDFLARED_VERSION}`,
      { headers: { Accept: "application/vnd.github+json", "User-Agent": "tunnelpup-build", ...auth } },
    );
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const { body = "", assets = [] } = await res.json();
    const notes = {};
    for (const m of body.matchAll(/^\s*(cloudflared-[\w.-]+):\s*([a-f0-9]{64})\s*$/gim)) {
      notes[m[1]] = m[2].toLowerCase();
    }
    const digests = {};
    for (const a of assets) {
      if (a.digest?.startsWith("sha256:")) digests[a.name] = a.digest.slice("sha256:".length).toLowerCase();
    }
    return { notes, digests };
  } catch (e) {
    console.warn(`! Could not fetch release checksums (${e.message}); skipping verification.`);
    return { notes: {}, digests: {} };
  }
}

function verify(data, expected, what) {
  if (!expected) return;
  const actual = createHash("sha256").update(data).digest("hex");
  if (actual !== expected) throw new Error(`Checksum mismatch for ${what}: expected ${expected}, got ${actual}`);
  console.log(`  sha256 verified: ${what}`);
}

async function download(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`Download failed: ${url} (HTTP ${res.status})`);
  return Buffer.from(await res.arrayBuffer());
}

async function fetchTarget(triple, sums) {
  const asset = ASSETS[triple];
  if (!asset) throw new Error(`Unsupported target: ${triple}. Supported: ${Object.keys(ASSETS).join(", ")}`);

  const isWindows = triple.includes("windows");
  const dest = join(OUT_DIR, `tunnelpup-cloudflared-${triple}${isWindows ? ".exe" : ""}`);
  const stamp = `${dest}.version`;
  if (existsSync(dest) && existsSync(stamp) && readFileSync(stamp, "utf8").trim() === CLOUDFLARED_VERSION) {
    console.log(`✓ ${triple}: already at ${CLOUDFLARED_VERSION}`);
    return;
  }

  console.log(`↓ ${triple}: ${asset}`);
  const buf = await download(`${BASE}/${asset}`);
  verify(buf, sums.digests[asset], `${asset} (GitHub)`);

  let binary = buf;
  if (asset.endsWith(".tgz")) {
    const tmp = mkdtempSync(join(tmpdir(), "cloudflared-"));
    const tgz = join(tmp, asset);
    writeFileSync(tgz, buf);
    execSync(`tar -xzf "${tgz}" -C "${tmp}"`);
    binary = readFileSync(join(tmp, "cloudflared"));
    rmSync(tmp, { recursive: true, force: true });
  }
  verify(binary, sums.notes[asset], `cloudflared from ${asset} (Cloudflare's release notes)`);

  mkdirSync(OUT_DIR, { recursive: true });
  writeFileSync(dest, binary);
  if (!isWindows) chmodSync(dest, 0o755);
  writeFileSync(stamp, CLOUDFLARED_VERSION);
  console.log(`  → ${dest}`);
}

const args = process.argv.slice(2);
const targets = args.includes("--all")
  ? Object.keys(ASSETS)
  : args.includes("--target")
    ? [args[args.indexOf("--target") + 1]]
    : [hostTriple()];

const sums = await fetchChecksums();
for (const t of targets) await fetchTarget(t, sums);
