import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { channelFromVersion, endpointFor } from "./update-channel-lib.mjs";

// Write a tauri.release.conf.json with release-only overrides.
//
// Tauri's --config flag merges the provided JSON on top of the base
// tauri.conf.json, so this file must contain ONLY the delta fields —
// not a copy of the base config.
//
// Release builds emit:
// 1. bundle.createUpdaterArtifacts = true so Tauri produces the .tar.gz
//    (or platform archive) and .sig during the build.
// 2. plugins.updater with pubkey + endpoint from env vars.
//    Both OMP_DESKTOP_UPDATER_PUBLIC_KEY and OMP_DESKTOP_UPDATER_ENDPOINT are required.
//
// Usage (CI):
//   OMP_DESKTOP_UPDATER_PUBLIC_KEY=... \
//   OMP_DESKTOP_UPDATER_ENDPOINT=https://github.com/<org>/omp-desktop/releases/download/omp-desktop-latest/latest.json \
//   node scripts/build-release-config.mjs
//
// Then:
//   pnpm tauri build --config src-tauri/tauri.release.conf.json
// with the same OMP_DESKTOP_UPDATER_* env vars so build.rs enables the plugin.

const outputConfigPath = resolve(
  process.cwd(),
  "src-tauri/tauri.release.conf.json",
);

const updaterPubkey = process.env.OMP_DESKTOP_UPDATER_PUBLIC_KEY;
// Channel from the release version being built (tag); endpoint precedence:
// explicit OMP_DESKTOP_UPDATER_ENDPOINT override → channel-derived from repo.
const releaseVersion = process.env.OMP_DESKTOP_RELEASE_VERSION ?? "";
const channel = channelFromVersion(releaseVersion);
const updaterEndpoint =
  process.env.OMP_DESKTOP_UPDATER_ENDPOINT ??
  (process.env.GITHUB_REPOSITORY
    ? endpointFor(process.env.GITHUB_REPOSITORY, channel)
    : undefined);

const missing = [];
if (!updaterPubkey) missing.push("OMP_DESKTOP_UPDATER_PUBLIC_KEY");
if (!updaterEndpoint)
  missing.push("OMP_DESKTOP_UPDATER_ENDPOINT (or GITHUB_REPOSITORY to derive)");
if (missing.length > 0) {
  console.error(
    `Error: required environment variable(s) missing: ${missing.join(", ")}`,
  );
  process.exit(1);
}

const releaseConfig = {
  bundle: {
    macOS: {
      minimumSystemVersion: "11.0",
    },
    createUpdaterArtifacts: true,
  },
  plugins: {
    updater: {
      pubkey: updaterPubkey,
      endpoints: [updaterEndpoint],
    },
  },
};

const pubkeyPreview =
  updaterPubkey.length > 24
    ? `${updaterPubkey.slice(0, 12)}…${updaterPubkey.slice(-8)}`
    : "(short key)";
console.log(`Update channel  -> ${channel} (version ${releaseVersion || "unknown"})`);
console.log(`Updater enabled -> ${updaterEndpoint}`);
console.log(`Pubkey prefix   -> ${pubkeyPreview}`);

writeFileSync(outputConfigPath, `${JSON.stringify(releaseConfig, null, 2)}\n`);
console.log(`Wrote ${outputConfigPath}`);
console.log(
  "Next: pnpm tauri build --config src-tauri/tauri.release.conf.json",
);
console.log(
  "(same OMP_DESKTOP_UPDATER_* env vars must be set so build.rs enables registration)",
);
