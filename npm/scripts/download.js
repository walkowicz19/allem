// Resolves the Allem binary for the current platform, downloading a prebuilt release asset
// from GitHub Releases on first use and caching it under the package's vendor/ dir.
//
// Used two ways:
//   - as a best-effort `postinstall` (pre-warms the cache; never fails the install)
//   - lazily from bin/allem.js on first run (so `npx allem` works even with --ignore-scripts)
//
// No third-party dependencies — Node built-ins only. Release assets are single binaries
// gzip-compressed (decompressed here with zlib), so no tar/unzip is required.

"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");
const zlib = require("zlib");

const pkg = require("../package.json");

const TAG = process.env.ALLEM_RELEASE_TAG || `v${pkg.version}`;
const BASE =
  process.env.ALLEM_DOWNLOAD_BASE ||
  `https://github.com/walkowicz19/allem/releases/download/${TAG}`;

// Maps Node platform-arch → the release asset name produced by .github/workflows/release.yml.
const ASSETS = {
  "linux-x64": "allem-linux-x64.gz",
  "darwin-x64": "allem-darwin-x64.gz",
  "darwin-arm64": "allem-darwin-arm64.gz",
  "win32-x64": "allem-win32-x64.exe.gz",
};

function assetName() {
  const key = `${process.platform}-${process.arch}`;
  const asset = ASSETS[key];
  if (!asset) {
    throw new Error(
      `unsupported platform '${key}'. Build from source instead: ` +
        `cargo install --git https://github.com/walkowicz19/allem allem-cli`
    );
  }
  return asset;
}

function binName() {
  return process.platform === "win32" ? "allem.exe" : "allem";
}

function vendorDir() {
  return path.join(__dirname, "..", "vendor");
}

function binPath() {
  return path.join(vendorDir(), binName());
}

// GET with redirect following (GitHub release downloads redirect to object storage).
function get(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "allem-npm" } }, (res) => {
        const { statusCode, headers } = res;
        if (statusCode >= 300 && statusCode < 400 && headers.location) {
          res.resume();
          resolve(get(headers.location));
          return;
        }
        if (statusCode !== 200) {
          res.resume();
          reject(new Error(`HTTP ${statusCode} for ${url}`));
          return;
        }
        resolve(res);
      })
      .on("error", reject);
  });
}

// Returns the path to a ready-to-run binary, downloading + decompressing it if absent.
async function ensureBinary() {
  const target = binPath();
  if (fs.existsSync(target)) {
    return target;
  }
  fs.mkdirSync(vendorDir(), { recursive: true });

  const url = `${BASE}/${assetName()}`;
  const res = await get(url);
  const tmp = `${target}.tmp`;

  await new Promise((resolve, reject) => {
    const out = fs.createWriteStream(tmp);
    res.on("error", reject);
    out.on("error", reject);
    out.on("finish", resolve);
    res.pipe(zlib.createGunzip()).on("error", reject).pipe(out);
  });

  fs.renameSync(tmp, target);
  if (process.platform !== "win32") {
    fs.chmodSync(target, 0o755);
  }
  return target;
}

module.exports = { ensureBinary, binPath, assetName };

// postinstall: best-effort pre-download. Never fail `npm install` over it.
if (require.main === module) {
  ensureBinary().catch((err) => {
    console.warn(
      `allem: could not pre-download the binary (${err.message}). ` +
        `It will be fetched on first run.`
    );
  });
}
