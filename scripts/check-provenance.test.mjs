import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepository, validateUpstreams } from "./check-provenance.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function baseline(overrides = {}) {
  return {
    schemaVersion: 1,
    desktop: {
      repository: "https://github.com/Po1nt9/omp-desktop.git",
      publicationState: "planned",
    },
    grokApp: {
      remote: "https://github.com/RongleCat/grok-app.git",
      importCommit: "d2a2563f19bba46cb67496d3b4ac821a31bceaed",
      importedAt: "2026-07-28",
      historyMode: "two-parent-merge",
    },
    omp: {
      officialRemote: "https://github.com/can1357/oh-my-pi.git",
      forkRemote: "https://github.com/Po1nt9/oh-my-pi.git",
      forkPublicationState: "planned",
      submodulePath: "runtime/oh-my-pi",
      pinnedCommit: "667111575ebba136dadfd6989379e7f67e0d40d9",
      officialBaseCommit: "667111575ebba136dadfd6989379e7f67e0d40d9",
    },
    ...overrides,
  };
}

test("requires exact Grok and OMP baselines", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "omp-provenance-"));
  await writeFile(path.join(directory, "upstreams.json"), JSON.stringify({ schemaVersion: 1 }));
  assert.throws(() => validateUpstreams(directory), /grokApp\.importCommit/);
});

test("requires publication records to remain honest while repositories are unpublished", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "omp-provenance-"));
  await writeFile(
    path.join(directory, "upstreams.json"),
    JSON.stringify(baseline({ desktop: { ...baseline().desktop, publicationState: "published" } })),
  );
  assert.throws(() => validateUpstreams(directory), /desktop\.publicationState/);
});

test("checks real remotes and gitlink while reporting publication blockers", () => {
  const result = checkRepository(root);
  assert.deepEqual(result.publicationConcerns, ["desktop.repository", "omp.forkRemote"]);
});
