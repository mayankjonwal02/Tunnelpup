#!/usr/bin/env node
// Writes public/third-party-licenses.txt: the licence notices of what TunnelPup is built from,
// the Rust crates (via cargo-about, set up in src-tauri/about.toml) and the npm packages bundled
// into its interface. Settings → Open-source licenses shows the file; the release workflow
// generates it before every build. cloudflared's licence is shown separately.
//
// Needs cargo-about: cargo install cargo-about --locked --features cli

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "public", "third-party-licenses.txt");
const RULE = "-".repeat(80);

// cargo-about writes to a file, not stdout: under PowerShell (the shell on Windows runners) it
// refuses redirected output, which PowerShell would re-encode.
const tmp = mkdtempSync(join(tmpdir(), "tunnelpup-licenses-"));
let rust;
try {
  const file = join(tmp, "rust.txt");
  execFileSync("cargo", ["about", "generate", "--output-file", file, "about.hbs"], {
    cwd: join(ROOT, "src-tauri"),
    stdio: ["ignore", "inherit", "inherit"],
  });
  rust = readFileSync(file, "utf8");
} finally {
  rmSync(tmp, { recursive: true, force: true });
}

// Dual-licensed packages are used under MIT, like the Rust crates.
function licenceText(dir) {
  const files = readdirSync(dir);
  const file =
    files.find((f) => /^licen[cs]e-mit/i.test(f)) ?? files.find((f) => /^(licen[cs]e|copying)(\.(md|txt))?$/i.test(f));
  return file ? readFileSync(join(dir, file), "utf8").trim() : null;
}

// The npm packages in the interface: the production dependencies and what they pull in.
const lock = JSON.parse(readFileSync(join(ROOT, "package-lock.json"), "utf8"));
const packages = Object.entries(lock.packages).filter(
  ([path, p]) => path.startsWith("node_modules/") && !p.dev && !p.devOptional,
);

// The Tauri plugin packages ship only an SPDX tag. Their licence is the Tauri project's, the
// same text that @tauri-apps/api includes.
const tauriText = licenceText(join(ROOT, "node_modules", "@tauri-apps", "api"));
const groups = new Map();
for (const [path, p] of packages) {
  const name = path.slice(path.lastIndexOf("node_modules/") + "node_modules/".length);
  const text = licenceText(join(ROOT, path)) ?? (name.startsWith("@tauri-apps/") ? tauriText : null);
  if (!text) throw new Error(`No licence text found for ${name}; add one to scripts/licenses.mjs.`);
  groups.set(text, [...(groups.get(text) ?? []), `${name} ${p.version}`]);
}
const npm = [...groups]
  .map(([text, names]) => `${RULE}\nUsed by:\n${names.map((n) => `  ${n}`).join("\n")}\n\n${text}\n`)
  .join("\n");

const notices = `Third-party software in TunnelPup
=================================

${rust.trim()}


npm packages
============

TunnelPup's interface is built with these npm packages.

${npm}`;
// One kind of line ending whatever the platform: a Windows checkout gives the template CRLF.
writeFileSync(OUT, notices.replace(/\r\n?/g, "\n"));
console.log(`Wrote ${OUT} (${packages.length} npm packages)`);
