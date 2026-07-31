// Single source of truth for update-channel derivation (AC-10.9).
// Channels are build-time identities derived from the version/tag string —
// see docs/superpowers/specs/2026-07-31-update-channels-design.md (D1/D2/D7).
//
// CLI (used by release.yml steps):
//   node scripts/update-channel-lib.mjs channel v1.1.0-beta.1
//   node scripts/update-channel-lib.mjs endpoint owner/repo v1.1.0-beta.1
//   node scripts/update-channel-lib.mjs prerelease v1.1.0-beta.1

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const CHANNELS = ["stable", "beta", "nightly"];

export const ROLLING_TAGS = {
  stable: "omp-desktop-latest",
  beta: "omp-desktop-beta",
  nightly: "omp-desktop-nightly",
};

export const MANIFEST_NAMES = {
  stable: "latest.json",
  beta: "beta.json",
  nightly: "nightly.json",
};

/** "v1.1.0-beta.1" → "beta"; "0.3.1-nightly" → "nightly"; "1.0.0" → "stable". */
export function channelFromVersion(version) {
  const v = String(version ?? "")
    .trim()
    .replace(/^[vV]/, "");
  const pre = v.includes("-") ? v.slice(v.indexOf("-") + 1).toLowerCase() : "";
  if (pre.includes("nightly")) return "nightly";
  if (pre.includes("beta")) return "beta";
  return "stable";
}

export function channelFromTag(tag) {
  return channelFromVersion(tag);
}

export function rollingTagFor(channel) {
  const tag = ROLLING_TAGS[channel];
  if (!tag) throw new Error(`unknown update channel: ${channel}`);
  return tag;
}

export function manifestNameFor(channel) {
  const name = MANIFEST_NAMES[channel];
  if (!name) throw new Error(`unknown update channel: ${channel}`);
  return name;
}

/** Feed endpoint for a repo + channel: the rolling-release manifest URL. */
export function endpointFor(repo, channel) {
  return `https://github.com/${repo}/releases/download/${rollingTagFor(channel)}/${manifestNameFor(channel)}`;
}

/** GitHub Release prerelease flag: everything except stable is a prerelease. */
export function isPrerelease(channel) {
  return channel !== "stable";
}

const isMain =
  process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);

if (isMain) {
  const [cmd, a, b] = process.argv.slice(2);
  try {
    if (cmd === "channel" && a) {
      console.log(channelFromTag(a));
    } else if (cmd === "endpoint" && a && b) {
      console.log(endpointFor(a, channelFromTag(b)));
    } else if (cmd === "prerelease" && a) {
      console.log(String(isPrerelease(channelFromTag(a))));
    } else {
      console.error(
        "usage: update-channel-lib.mjs channel <tag> | endpoint <owner/repo> <tag> | prerelease <tag>",
      );
      process.exit(1);
    }
  } catch (err) {
    console.error(String(err?.message ?? err));
    process.exit(1);
  }
}
