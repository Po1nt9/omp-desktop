import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";
import {
  channelFromTag,
  channelFromVersion,
  endpointFor,
  isPrerelease,
  manifestNameFor,
  rollingTagFor,
} from "./update-channel-lib.mjs";

test("channelFromVersion maps prerelease suffixes", () => {
  assert.equal(channelFromVersion("1.0.0"), "stable");
  assert.equal(channelFromVersion("v1.0.0"), "stable");
  assert.equal(channelFromVersion("0.3.1-nightly"), "nightly");
  assert.equal(channelFromVersion("1.1.0-nightly.20260801"), "nightly");
  assert.equal(channelFromVersion("1.1.0-beta.1"), "beta");
  assert.equal(channelFromVersion("v1.1.0-beta.2"), "beta");
  assert.equal(channelFromVersion(""), "stable");
});

test("channelFromTag mirrors channelFromVersion", () => {
  assert.equal(channelFromTag("v0.3.1-nightly"), "nightly");
});

test("rolling tag + manifest per channel (stable keeps legacy feed)", () => {
  assert.equal(rollingTagFor("stable"), "omp-desktop-latest");
  assert.equal(manifestNameFor("stable"), "latest.json");
  assert.equal(rollingTagFor("beta"), "omp-desktop-beta");
  assert.equal(manifestNameFor("beta"), "beta.json");
  assert.equal(rollingTagFor("nightly"), "omp-desktop-nightly");
  assert.equal(manifestNameFor("nightly"), "nightly.json");
  assert.throws(() => rollingTagFor("canary"), /unknown update channel/);
});

test("endpointFor builds the rolling-release manifest URL", () => {
  assert.equal(
    endpointFor("owner/omp-desktop", "nightly"),
    "https://github.com/owner/omp-desktop/releases/download/omp-desktop-nightly/nightly.json",
  );
  assert.equal(
    endpointFor("owner/omp-desktop", "stable"),
    "https://github.com/owner/omp-desktop/releases/download/omp-desktop-latest/latest.json",
  );
});

test("isPrerelease: everything except stable", () => {
  assert.equal(isPrerelease("stable"), false);
  assert.equal(isPrerelease("beta"), true);
  assert.equal(isPrerelease("nightly"), true);
});

test("CLI prints channel / endpoint / prerelease for CI steps", () => {
  const run = (...args) =>
    execFileSync("node", ["scripts/update-channel-lib.mjs", ...args], {
      encoding: "utf8",
    }).trim();
  assert.equal(run("channel", "v1.1.0-beta.1"), "beta");
  assert.equal(
    run("endpoint", "owner/omp-desktop", "v1.1.0-beta.1"),
    "https://github.com/owner/omp-desktop/releases/download/omp-desktop-beta/beta.json",
  );
  assert.equal(run("prerelease", "v1.0.0"), "false");
  assert.equal(run("prerelease", "v0.3.1-nightly"), "true");
});

const runAssembleDerived = (extraEnv) =>
  execFileSync("bash", ["scripts/assemble-updater-manifest.sh"], {
    env: { ...process.env, PRINT_DERIVED: "1", ...extraEnv },
    encoding: "utf8",
  });

test("assemble-updater-manifest PRINT_DERIVED maps all three channels", () => {
  const stable = runAssembleDerived({ CHANNEL: "stable", ROLLING_TAG: "" });
  assert.match(stable, /CHANNEL=stable/);
  assert.match(stable, /ROLLING_TAG=omp-desktop-latest/);
  assert.match(stable, /MANIFEST_NAME=latest\.json/);
  const beta = runAssembleDerived({ CHANNEL: "beta", ROLLING_TAG: "" });
  assert.match(beta, /ROLLING_TAG=omp-desktop-beta/);
  assert.match(beta, /MANIFEST_NAME=beta\.json/);
  const nightly = runAssembleDerived({ CHANNEL: "nightly", ROLLING_TAG: "" });
  assert.match(nightly, /ROLLING_TAG=omp-desktop-nightly/);
  assert.match(nightly, /MANIFEST_NAME=nightly\.json/);
});

test("assemble-updater-manifest rejects unknown CHANNEL (fail-closed)", () => {
  assert.throws(
    () => runAssembleDerived({ CHANNEL: "canary", ROLLING_TAG: "" }),
    /unknown CHANNEL/,
  );
});
