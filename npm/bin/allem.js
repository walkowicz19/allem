#!/usr/bin/env node
// Thin launcher: ensure the platform binary is present (download on first run), then exec it
// with the user's arguments and stdio inherited (so `allem mcp` works over stdio).

"use strict";

const { spawnSync } = require("child_process");
const { ensureBinary } = require("../scripts/download.js");

(async () => {
  let binary;
  try {
    binary = await ensureBinary();
  } catch (err) {
    console.error(`allem: failed to obtain the binary: ${err.message}`);
    process.exit(1);
  }

  const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    console.error(`allem: ${result.error.message}`);
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
})();
