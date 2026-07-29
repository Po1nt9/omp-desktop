import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const GROK_BASE = "d2a2563f19bba46cb67496d3b4ac821a31bceaed";
const OMP_BASE = "667111575ebba136dadfd6989379e7f67e0d40d9";
const DESKTOP_REPOSITORY = "https://github.com/Po1nt9/omp-desktop.git";
const GROK_REMOTE = "https://github.com/RongleCat/grok-app.git";
const OMP_FORK_REMOTE = "https://github.com/Po1nt9/oh-my-pi.git";
const OMP_OFFICIAL_REMOTE = "https://github.com/can1357/oh-my-pi.git";
const OMP_PATH = "runtime/oh-my-pi";

function requireValue(actual, expected, field) {
  if (actual !== expected) throw new Error(`${field} must be ${expected}`);
}

function requirePublicationState(actual, field) {
  if (actual !== "planned" && actual !== "published") {
    throw new Error(`${field} must be planned or published`);
  }
}

function git(directory, args) {
  return execFileSync("git", ["-C", directory, ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function optionalRemote(directory, name) {
  try {
    return git(directory, ["remote", "get-url", name]);
  } catch {
    return null;
  }
}

export function validateUpstreams(directory) {
  const data = JSON.parse(fs.readFileSync(path.join(directory, "upstreams.json"), "utf8"));

  requireValue(data.grokApp?.importCommit, GROK_BASE, "grokApp.importCommit");
  requireValue(data.schemaVersion, 1, "schemaVersion");
  requireValue(data.desktop?.repository, DESKTOP_REPOSITORY, "desktop.repository");
  requirePublicationState(data.desktop?.publicationState, "desktop.publicationState");
  requireValue(data.grokApp?.remote, GROK_REMOTE, "grokApp.remote");
  requireValue(data.grokApp?.importedAt, "2026-07-28", "grokApp.importedAt");
  requireValue(data.grokApp?.historyMode, "two-parent-merge", "grokApp.historyMode");
  requireValue(data.omp?.officialRemote, OMP_OFFICIAL_REMOTE, "omp.officialRemote");
  requireValue(data.omp?.forkRemote, OMP_FORK_REMOTE, "omp.forkRemote");
  requirePublicationState(data.omp?.forkPublicationState, "omp.forkPublicationState");
  requireValue(data.omp?.submodulePath, OMP_PATH, "omp.submodulePath");
  requireValue(data.omp?.pinnedCommit, OMP_BASE, "omp.pinnedCommit");
  requireValue(data.omp?.officialBaseCommit, OMP_BASE, "omp.officialBaseCommit");

  return data;
}

export function validatePatchLedger(directory) {
  const data = JSON.parse(fs.readFileSync(path.join(directory, "omp-patches.json"), "utf8"));
  requireValue(data.schemaVersion, 1, "omp-patches.schemaVersion");
  requireValue(data.baseCommit, OMP_BASE, "omp-patches.baseCommit");
  if (!Array.isArray(data.patches)) throw new Error("omp-patches.patches must be an array");
  const shaPattern = /^[0-9a-f]{40}$/;
  for (const [index, patch] of data.patches.entries()) {
    const at = `omp-patches.patches[${index}]`;
    if (typeof patch.id !== "string" || patch.id.length === 0) throw new Error(`${at}.id must be a non-empty string`);
    if (typeof patch.branch !== "string" || patch.branch.length === 0) throw new Error(`${at}.branch must be a non-empty string`);
    if (typeof patch.description !== "string" || patch.description.length === 0) throw new Error(`${at}.description must be a non-empty string`);
    if (typeof patch.plan !== "string" || patch.plan.length === 0) throw new Error(`${at}.plan must be a non-empty string`);
    if (typeof patch.commit !== "string" || !shaPattern.test(patch.commit)) throw new Error(`${at}.commit must be a 40-character lowercase hex SHA`);
  }
  return data;
}

export function checkRepository(root) {
  const provenanceDirectory = path.join(root, "provenance");
  const data = validateUpstreams(provenanceDirectory);
  const patchData = validatePatchLedger(provenanceDirectory);

  const expectedCommit =
    patchData.patches.length > 0
      ? patchData.patches[patchData.patches.length - 1].commit
      : data.omp.pinnedCommit;

  const superprojectOrigin = optionalRemote(root, "origin");
  if (data.desktop.publicationState === "published") {
    requireValue(
      superprojectOrigin,
      data.desktop.repository,
      "superproject origin while desktop publication is published",
    );
  } else {
    requireValue(superprojectOrigin, null, "superproject origin while desktop publication is planned");
  }
  requireValue(git(root, ["remote", "get-url", "grok-app-upstream"]), data.grokApp.remote, "grok-app-upstream remote");
  git(root, ["merge-base", "--is-ancestor", data.grokApp.importCommit, "HEAD"]);

  const gitlink = git(root, ["ls-tree", "HEAD", "--", data.omp.submodulePath]);
  const match = gitlink.match(/^160000 commit ([0-9a-f]{40})\t/);
  if (!match) throw new Error(`${data.omp.submodulePath} must be a committed gitlink`);
  requireValue(match[1], expectedCommit, "submodule gitlink");

  const submodule = path.join(root, data.omp.submodulePath);
  const submoduleHead = git(submodule, ["rev-parse", "HEAD"]);
  requireValue(submoduleHead, match[1], "submodule HEAD");
  if (patchData.patches.length > 0) {
    try {
      execFileSync("git", ["-C", submodule, "merge-base", "--is-ancestor", patchData.baseCommit, submoduleHead], {
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch {
      throw new Error("submodule HEAD must be a descendant of the patch base commit");
    }
  }
  requireValue(git(submodule, ["remote", "get-url", "origin"]), data.omp.forkRemote, "submodule origin");
  requireValue(git(submodule, ["remote", "get-url", "upstream"]), data.omp.officialRemote, "submodule upstream");
  requireValue(
    git(root, ["config", "--file", ".gitmodules", "--get", `submodule.${data.omp.submodulePath}.url`]),
    data.omp.forkRemote,
    ".gitmodules submodule URL",
  );

  const publicationConcerns = [];
  if (data.desktop.publicationState === "planned") publicationConcerns.push("desktop.repository");
  if (data.omp.forkPublicationState === "planned") publicationConcerns.push("omp.forkRemote");
  return { publicationConcerns };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = checkRepository(process.cwd());
  console.log("Provenance checks passed: frozen records, remotes, gitlink, and submodule checkout match.");
  if (result.publicationConcerns.length > 0) {
    console.log(`Publication pending: ${result.publicationConcerns.join(", ")}`);
  } else {
    console.log("Publication verified: desktop.repository and omp.forkRemote are published.");
  }
}
