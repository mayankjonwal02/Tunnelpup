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
// Bump CLOUDFLARED_VERSION when you ship an app update with a newer connector, and update
// SHA256 with it. Each value is the SHA-256 of the release file, which GitHub shows next to the
// file on the release page. Cross-check it against the checksums in Cloudflare's release notes:
// for the plain binaries they're the same, for the macOS .tgz files they're the binary inside.

import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const CLOUDFLARED_VERSION = "2026.10.0";

// A download that doesn't match is never used.
const SHA256 = {
  "cloudflared-windows-amd64.exe": "86aee4017b26625cee8484c113558f48effa4cd47f7aa05fcf425604e5d2b23c",
  "cloudflared-darwin-amd64.tgz": "903845b81828c8cb3c5d13d816a2de71c06a3da5785469df8eb0e1b736d92f9f",
  "cloudflared-darwin-arm64.tgz": "a2f79ff7b9420aa537d74af239f376da170bbabeb529aec416002adac6a72e70",
  "cloudflared-linux-amd64": "d33ff2d14475178d2012c2c56beba87389ac5ded27649519f198a7d3134a99db",
  "cloudflared-linux-arm64": "e6422b9d4f72d3194bc5a38676f13667c06666523217b842a877d72a80b5ac08",
};

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

function verify(data, asset) {
  const expected = SHA256[asset];
  if (!expected) throw new Error(`No SHA-256 pinned for ${asset}; add it to SHA256 in this script.`);
  const actual = createHash("sha256").update(data).digest("hex");
  if (actual !== expected) throw new Error(`Checksum mismatch for ${asset}: expected ${expected}, got ${actual}`);
  console.log(`  sha256 verified: ${asset}`);
}

async function download(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`Download failed: ${url} (HTTP ${res.status})`);
  return Buffer.from(await res.arrayBuffer());
}

async function fetchTarget(triple) {
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
  verify(buf, asset);

  let binary = buf;
  if (asset.endsWith(".tgz")) {
    const tmp = mkdtempSync(join(tmpdir(), "cloudflared-"));
    const tgz = join(tmp, asset);
    writeFileSync(tgz, buf);
    execSync(`tar -xzf "${tgz}" -C "${tmp}"`);
    binary = readFileSync(join(tmp, "cloudflared"));
    rmSync(tmp, { recursive: true, force: true });
  }

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

for (const t of targets) await fetchTarget(t);
