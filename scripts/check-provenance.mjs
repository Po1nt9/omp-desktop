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
  requireValue(data.desktop?.publicationState, "planned", "desktop.publicationState");
  requireValue(data.grokApp?.remote, GROK_REMOTE, "grokApp.remote");
  requireValue(data.grokApp?.importedAt, "2026-07-28", "grokApp.importedAt");
  requireValue(data.grokApp?.historyMode, "two-parent-merge", "grokApp.historyMode");
  requireValue(data.omp?.officialRemote, OMP_OFFICIAL_REMOTE, "omp.officialRemote");
  requireValue(data.omp?.forkRemote, OMP_FORK_REMOTE, "omp.forkRemote");
  requireValue(data.omp?.forkPublicationState, "planned", "omp.forkPublicationState");
  requireValue(data.omp?.submodulePath, OMP_PATH, "omp.submodulePath");
  requireValue(data.omp?.pinnedCommit, OMP_BASE, "omp.pinnedCommit");
  requireValue(data.omp?.officialBaseCommit, OMP_BASE, "omp.officialBaseCommit");

  return data;
}

function validatePatchLedger(directory) {
  const data = JSON.parse(fs.readFileSync(path.join(directory, "omp-patches.json"), "utf8"));
  requireValue(data.schemaVersion, 1, "omp-patches.schemaVersion");
  requireValue(data.baseCommit, OMP_BASE, "omp-patches.baseCommit");
  if (!Array.isArray(data.patches)) throw new Error("omp-patches.patches must be an array");
}

export function checkRepository(root) {
  const provenanceDirectory = path.join(root, "provenance");
  const data = validateUpstreams(provenanceDirectory);
  validatePatchLedger(provenanceDirectory);

  requireValue(optionalRemote(root, "origin"), null, "superproject origin while desktop publication is planned");
  requireValue(git(root, ["remote", "get-url", "grok-app-upstream"]), data.grokApp.remote, "grok-app-upstream remote");
  git(root, ["merge-base", "--is-ancestor", data.grokApp.importCommit, "HEAD"]);

  const gitlink = git(root, ["ls-tree", "HEAD", "--", data.omp.submodulePath]);
  const match = gitlink.match(/^160000 commit ([0-9a-f]{40})\t/);
  if (!match) throw new Error(`${data.omp.submodulePath} must be a committed gitlink`);
  requireValue(match[1], data.omp.pinnedCommit, "submodule gitlink");

  const submodule = path.join(root, data.omp.submodulePath);
  requireValue(git(submodule, ["rev-parse", "HEAD"]), data.omp.pinnedCommit, "submodule HEAD");
  requireValue(git(submodule, ["remote", "get-url", "origin"]), data.omp.forkRemote, "submodule origin");
  requireValue(git(submodule, ["remote", "get-url", "upstream"]), data.omp.officialRemote, "submodule upstream");
  requireValue(
    git(root, ["config", "--file", ".gitmodules", "--get", `submodule.${data.omp.submodulePath}.url`]),
    data.omp.forkRemote,
    ".gitmodules submodule URL",
  );

  return { publicationConcerns: ["desktop.repository", "omp.forkRemote"] };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = checkRepository(process.cwd());
  console.log("Provenance checks passed: frozen records, remotes, gitlink, and submodule checkout match.");
  console.log(`Publication pending: ${result.publicationConcerns.join(", ")}`);
}
