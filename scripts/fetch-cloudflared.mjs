#!/usr/bin/env node
// Downloads the official cloudflared binary into src-tauri/binaries/ with the
// file name Tauri's sidecar system expects: porthop-cloudflared-<rust-target-triple>[.exe]
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
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
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

async function fetchChecksums() {
  // Cloudflare lists SHA256 checksums in the release notes. Best effort: if the
  // API is unreachable or rate-limited we warn and continue.
  try {
    // In CI, GITHUB_TOKEN avoids the low anonymous rate limit shared by all runners.
    const auth = process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {};
    const res = await fetch(
      `https://api.github.com/repos/cloudflare/cloudflared/releases/tags/${CLOUDFLARED_VERSION}`,
      { headers: { Accept: "application/vnd.github+json", "User-Agent": "porthop-build", ...auth } },
    );
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const { body = "" } = await res.json();
    const sums = {};
    for (const m of body.matchAll(/^\s*(cloudflared-[\w.-]+):\s*([a-f0-9]{64})\s*$/gim)) {
      sums[m[1]] = m[2].toLowerCase();
    }
    return sums;
  } catch (e) {
    console.warn(`! Could not fetch release checksums (${e.message}); skipping verification.`);
    return {};
  }
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
  const dest = join(OUT_DIR, `porthop-cloudflared-${triple}${isWindows ? ".exe" : ""}`);
  const stamp = `${dest}.version`;
  if (existsSync(dest) && existsSync(stamp) && readFileSync(stamp, "utf8").trim() === CLOUDFLARED_VERSION) {
    console.log(`✓ ${triple}: already at ${CLOUDFLARED_VERSION}`);
    return;
  }

  console.log(`↓ ${triple}: ${asset}`);
  const buf = await download(`${BASE}/${asset}`);

  const expected = sums[asset];
  if (expected) {
    const actual = createHash("sha256").update(buf).digest("hex");
    if (actual !== expected) throw new Error(`Checksum mismatch for ${asset}: expected ${expected}, got ${actual}`);
    console.log(`  sha256 verified`);
  }

  mkdirSync(OUT_DIR, { recursive: true });
  if (asset.endsWith(".tgz")) {
    const tmp = mkdtempSync(join(tmpdir(), "cloudflared-"));
    const tgz = join(tmp, asset);
    writeFileSync(tgz, buf);
    execSync(`tar -xzf "${tgz}" -C "${tmp}"`);
    renameSync(join(tmp, "cloudflared"), dest);
    rmSync(tmp, { recursive: true, force: true });
  } else {
    writeFileSync(dest, buf);
  }
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
