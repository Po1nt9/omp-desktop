import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepository, validatePatchLedger, validateUpstreams } from "./check-provenance.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const submodulePath = "runtime/oh-my-pi";
const submoduleRoot = path.join(root, submodulePath);
const grokCommit = "d2a2563f19bba46cb67496d3b4ac821a31bceaed";
const ompCommit = "667111575ebba136dadfd6989379e7f67e0d40d9";
const ompParentCommit = "59619623e1eeb7c290649eeaf3a269284ce8adef";
const desktopRemote = "https://github.com/Po1nt9/omp-desktop.git";
const grokRemote = "https://github.com/RongleCat/grok-app.git";
const ompForkRemote = "https://github.com/Po1nt9/oh-my-pi.git";
const ompOfficialRemote = "https://github.com/can1357/oh-my-pi.git";

function baseline() {
  return {
    schemaVersion: 1,
    desktop: {
      repository: desktopRemote,
      publicationState: "planned",
    },
    grokApp: {
      remote: grokRemote,
      importCommit: grokCommit,
      importedAt: "2026-07-28",
      historyMode: "two-parent-merge",
    },
    omp: {
      officialRemote: ompOfficialRemote,
      forkRemote: ompForkRemote,
      forkPublicationState: "planned",
      submodulePath,
      pinnedCommit: ompCommit,
      officialBaseCommit: ompCommit,
    },
  };
}

function runGit(directory, ...args) {
  return execFileSync("git", ["-C", directory, ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

async function makeTempDirectory(t) {
  const directory = await mkdtemp(path.join(tmpdir(), "omp-provenance-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

async function writeUpstreams(t, mutate) {
  const directory = await makeTempDirectory(t);
  const data = baseline();
  mutate(data);
  await writeFile(path.join(directory, "upstreams.json"), JSON.stringify(data));
  return directory;
}

async function makeRepositoryFixture(t) {
  const directory = await makeTempDirectory(t);
  const fixture = path.join(directory, "repository");

  execFileSync("git", ["clone", "--no-recurse-submodules", root, fixture], { stdio: "pipe" });
  runGit(fixture, "remote", "remove", "origin");
  runGit(fixture, "remote", "add", "grok-app-upstream", grokRemote);
  runGit(
    fixture,
    "-c",
    "protocol.file.allow=always",
    "-c",
    `submodule.${submodulePath}.url=${submoduleRoot}`,
    "submodule",
    "update",
    "--init",
  );

  const fixtureSubmodule = path.join(fixture, submodulePath);
  runGit(fixtureSubmodule, "remote", "set-url", "origin", ompForkRemote);
  runGit(fixtureSubmodule, "remote", "add", "upstream", ompOfficialRemote);
  return { fixture, fixtureSubmodule };
}

test("rejects an incorrect Grok import commit", async (t) => {
  const directory = await writeUpstreams(t, (data) => {
    data.grokApp.importCommit = "0".repeat(40);
  });
  assert.throws(() => validateUpstreams(directory), /grokApp\.importCommit/);
});

test("rejects an incorrect OMP pinned commit with the correct Grok commit", async (t) => {
  const directory = await writeUpstreams(t, (data) => {
    data.omp.pinnedCommit = "0".repeat(40);
  });
  assert.throws(() => validateUpstreams(directory), /omp\.pinnedCommit/);
});

test("rejects an incorrect OMP official base commit", async (t) => {
  const directory = await writeUpstreams(t, (data) => {
    data.omp.officialBaseCommit = "0".repeat(40);
  });
  assert.throws(() => validateUpstreams(directory), /omp\.officialBaseCommit/);
});

test("rejects an incorrect patch ledger base commit", async (t) => {
  const directory = await makeTempDirectory(t);
  await writeFile(
    path.join(directory, "omp-patches.json"),
    JSON.stringify({ schemaVersion: 1, baseCommit: "0".repeat(40), patches: [] }),
  );
  assert.throws(() => validatePatchLedger(directory), /omp-patches\.baseCommit/);
});

test("requires publication records to remain honest while repositories are unpublished", async (t) => {
  const directory = await writeUpstreams(t, (data) => {
    data.desktop.publicationState = "published";
  });
  assert.throws(() => validateUpstreams(directory), /desktop\.publicationState/);
});

test("accepts a network-free local repository fixture", async (t) => {
  const { fixture } = await makeRepositoryFixture(t);
  const result = checkRepository(fixture);
  assert.deepEqual(result.publicationConcerns, ["desktop.repository", "omp.forkRemote"]);
});

test("rejects an incorrect committed gitlink", async (t) => {
  const { fixture } = await makeRepositoryFixture(t);
  runGit(fixture, "config", "user.name", "Provenance Test");
  runGit(fixture, "config", "user.email", "provenance@example.invalid");
  runGit(fixture, "update-index", "--cacheinfo", `160000,${ompParentCommit},${submodulePath}`);
  runGit(fixture, "commit", "-m", "test wrong gitlink");
  assert.throws(() => checkRepository(fixture), /submodule gitlink/);
});

test("rejects a submodule checkout that differs from the gitlink", async (t) => {
  const { fixture, fixtureSubmodule } = await makeRepositoryFixture(t);
  runGit(fixtureSubmodule, "checkout", "--detach", ompParentCommit);
  assert.throws(() => checkRepository(fixture), /submodule HEAD/);
});

test("rejects an incorrect critical remote", async (t) => {
  const { fixture } = await makeRepositoryFixture(t);
  runGit(fixture, "remote", "set-url", "grok-app-upstream", "https://example.invalid/grok-app.git");
  assert.throws(() => checkRepository(fixture), /grok-app-upstream remote/);
});

test("rejects a missing critical remote", async (t) => {
  const { fixture, fixtureSubmodule } = await makeRepositoryFixture(t);
  runGit(fixtureSubmodule, "remote", "remove", "upstream");
  assert.throws(() => checkRepository(fixture));
});

test("rejects an incorrect .gitmodules URL", async (t) => {
  const { fixture } = await makeRepositoryFixture(t);
  runGit(
    fixture,
    "config",
    "--file",
    ".gitmodules",
    `submodule.${submodulePath}.url`,
    "https://example.invalid/oh-my-pi.git",
  );
  assert.throws(() => checkRepository(fixture), /\.gitmodules submodule URL/);
});

test("rejects an unexpected superproject origin while publication is planned", async (t) => {
  const { fixture } = await makeRepositoryFixture(t);
  runGit(fixture, "remote", "add", "origin", desktopRemote);
  assert.throws(() => checkRepository(fixture), /superproject origin while desktop publication is planned/);
});
