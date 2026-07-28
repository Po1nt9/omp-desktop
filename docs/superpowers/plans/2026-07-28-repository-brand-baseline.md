# Repository and Brand Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a reproducible OMP Desktop repository based on the exact Grok App source snapshot, pin the OMP Fork as a submodule, remove Grok App-specific branding and runtime coupling, and leave a buildable fail-closed desktop shell with auditable provenance and license inputs.

**Architecture:** Preserve the complete Grok App upstream history by connecting the frozen design root commit to the exact source baseline with a two-parent merge. Keep standard ACP framing and reusable UI/platform code, but replace every Grok-specific execution path with one centralized `runtime_unavailable` state; do not invent OMP protocol methods before Plan 2. Enforce the boundary with executable provenance, branding, and legal-policy checks.

**Tech Stack:** Git/GitHub CLI, React 19, TypeScript, Vite, pnpm, Tauri 2, Rust, Node.js policy scripts, JSON/TOML/YAML metadata.

## Global Constraints

- Product name is `OMP Desktop`; every user-visible brand abbreviation is `OMP`.
- Lowercase `omp` is allowed only in commands, executable names, paths, URLs, protocol namespaces, package/crate identifiers, and other technical identifiers.
- Remove Grok App-specific CLI, auth, account, quota, direct xAI network, `GROK_HOME`, `.grok`, and `_x.ai/*` coupling.
- Preserve OMP Runtime-provided xAI Provider names, Grok model names, Provider endpoints, auth methods, and redacted raw Provider errors as structured runtime data.
- Do not rename `_x.ai/*` to `_omp/*`; the versioned OMP Desktop Extension Protocol belongs to Plan 2.
- Do not hardcode a replacement model catalog or claim the OMP Runtime is connected in Plan 1.
- With no runtime connected, every Agent execution path must fail closed with the stable reason `runtime_unavailable`.
- The application version remains `0.1.9` during Plan 1.
- The bundle identifier is `io.github.po1nt9.omp-desktop`.
- The Grok App source baseline is exactly `d2a2563f19bba46cb67496d3b4ac821a31bceaed`.
- The OMP official baseline is exactly `667111575ebba136dadfd6989379e7f67e0d40d9`.
- Creating or pushing GitHub repositories is an external publication action and requires separate user confirmation immediately before execution.
- Plan 1 does not build or register a Tauri sidecar, implement a Supervisor, migrate credentials, define OMP extensions, or generate final artifact SBOMs.

---

## File and Module Map

**Create**

- `provenance/README.md` — upstream and patch-governance policy.
- `provenance/upstreams.json` — machine-readable pinned upstream graph.
- `provenance/omp-patches.json` — auditable OMP Fork patch ledger.
- `scripts/check-provenance.mjs` — validates remotes, ancestry, submodule commit, and metadata.
- `scripts/check-provenance.test.mjs` — deterministic provenance checker tests.
- `scripts/brand-policy.mjs` — prohibited patterns and precise allowlist entries.
- `scripts/check-brand-policy.mjs` — repository brand/runtime-coupling scanner.
- `scripts/check-brand-policy.test.mjs` — allowed and denied fixture tests.
- `testdata/brand-policy/allowed/provider-xai.json` — legal Provider identity fixture.
- `testdata/brand-policy/allowed/model-grok.json` — legal model identity fixture.
- `testdata/brand-policy/denied/app-title-grok.json` — prohibited product-brand fixture.
- `testdata/brand-policy/denied/direct-auth-xai.json` — prohibited direct-auth fixture.
- `testdata/brand-policy/denied/private-method-xai.json` — prohibited private protocol fixture.
- `testdata/brand-policy/denied/lowercase-brand.json` — prohibited user-visible lowercase OMP fixture.
- `src/lib/runtimeAvailability.ts` — single fail-closed frontend runtime state.
- `src/lib/runtimeAvailability.test.ts` — runtime state contract tests.
- `src-tauri/src/runtime_availability.rs` — single fail-closed Host runtime state.
- `src-tauri/icons/omp-mark.svg` — original OMP master artwork.
- `docs/upstream-history/grok-app/README.md` — immutable-history disclaimer.
- `third-party/inventory.json` — source-level dependency and resource inventory.
- `third-party/policy.toml` — accepted license policy and explicit exceptions.
- `sbom/inputs.json` — final-SBOM input manifest for Plan 9.
- `THIRD_PARTY_NOTICES` — aggregated source-distribution notices.
- `scripts/check-legal-baseline.mjs` — validates legal and SBOM inputs.
- `scripts/check-legal-baseline.test.mjs` — legal checker tests.

**Modify**

- `.gitignore`, `.gitmodules`, `package.json`, `pnpm-lock.yaml`.
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, Tauri manifests, `Info.plist`, build and updater metadata.
- `src-tauri/src/acp_client.rs`, `session_manager.rs`, `commands.rs`, `lib.rs`, `paths.rs` and registration tests.
- Frontend API, settings, setup, account, model, runtime, tray, notification, and locale surfaces.
- CI/release workflows and build/update/icon scripts.
- Current README and governance documents.

**Delete**

- Grok CLI install/probe/update/session modules and their product-specific tests.
- Grok account/profile/quota modules and direct xAI voice credential/network modules.
- `_x.ai/*` fixtures and production bindings.
- Deprecated `remote-bridge/` package after verifying no build reference remains.
- SuperGrok-specific components and artwork.

---

### Task 1: Connect the Frozen Design Commit to the Exact Grok App History

**Files:**
- Verify: `.gitignore`
- Verify: `docs/superpowers/specs/2026-07-28-omp-desktop-design.md`
- Generated outside repository: `/Users/po1nt9/Github/grok-app-pre-history-20260728.bundle`
- Generated outside repository: `/Users/po1nt9/Github/grok-app-working-tree-20260728.tar.gz`

**Interfaces:**
- Consumes: root commit `05e2aaa96bbdeb68d9740d102ccb1723e26673ed`; source baseline `d2a2563f19bba46cb67496d3b4ac821a31bceaed`.
- Produces: `main` whose first parent is the Grok baseline and second parent is the frozen design commit; `grok-app-upstream` remote; archive branch and baseline tags.

- [ ] **Step 1: Prove the starting state before touching refs**

Run:

```bash
test "$(git rev-parse HEAD)" = "05e2aaa96bbdeb68d9740d102ccb1723e26673ed"
test "$(git branch --show-current)" = "docs/omp-desktop-design"
test -z "$(git diff --name-only)"
test -z "$(git diff --cached --name-only)"
```

Expected: all commands exit `0`; `git status --short` contains only the untracked upstream source tree.

- [ ] **Step 2: Create and verify both recovery artifacts**

Run:

```bash
git bundle create /Users/po1nt9/Github/grok-app-pre-history-20260728.bundle --all
tar --exclude='./.git' -czf /Users/po1nt9/Github/grok-app-working-tree-20260728.tar.gz .
git bundle verify /Users/po1nt9/Github/grok-app-pre-history-20260728.bundle
tar -tzf /Users/po1nt9/Github/grok-app-working-tree-20260728.tar.gz >/dev/null
```

Expected: bundle verification reports `The bundle records a complete history`; tar verification exits `0`.

- [ ] **Step 3: Add and fetch the read-only Grok App upstream**

Run:

```bash
git remote add grok-app-upstream https://github.com/RongleCat/grok-app.git
git fetch grok-app-upstream --prune --tags
git cat-file -e d2a2563f19bba46cb67496d3b4ac821a31bceaed^{commit}
git merge-base --is-ancestor d2a2563f19bba46cb67496d3b4ac821a31bceaed grok-app-upstream/main
```

Expected: all commands exit `0`; no source files change.

- [ ] **Step 4: Create immutable local references**

Run:

```bash
git branch archive/pre-upstream-design 05e2aaa96bbdeb68d9740d102ccb1723e26673ed
git tag -a archive/pre-upstream-design-20260728 05e2aaa96bbdeb68d9740d102ccb1723e26673ed -m "Archive pre-upstream OMP design root commit"
git tag -a baseline/grok-app-d2a2563 d2a2563f19bba46cb67496d3b4ac821a31bceaed -m "Pinned grok-app source baseline for OMP Desktop"
```

Expected: `git show-ref` lists the archive branch and both annotated tags.

- [ ] **Step 5: Build the two-parent merge in an isolated worktree**

Run:

```bash
git worktree add -b integrate/omp-upstream-history /Users/po1nt9/Github/grok-app-history-integration d2a2563f19bba46cb67496d3b4ac821a31bceaed
git -C /Users/po1nt9/Github/grok-app-history-integration merge --no-ff --no-commit --allow-unrelated-histories archive/pre-upstream-design
git -C /Users/po1nt9/Github/grok-app-history-integration checkout --theirs -- .gitignore
mkdir -p /Users/po1nt9/Github/grok-app-history-integration/docs/superpowers/plans
cp /Users/po1nt9/Github/grok-app-main/docs/superpowers/plans/2026-07-28-repository-brand-baseline.md /Users/po1nt9/Github/grok-app-history-integration/docs/superpowers/plans/2026-07-28-repository-brand-baseline.md
git -C /Users/po1nt9/Github/grok-app-history-integration add .gitignore docs/superpowers/specs/2026-07-28-omp-desktop-design.md docs/superpowers/plans/2026-07-28-repository-brand-baseline.md
git -C /Users/po1nt9/Github/grok-app-history-integration commit -m "merge: connect OMP design with grok-app upstream history"
```

Expected: the merge pauses only for the `.gitignore` add/add conflict; the final commit has two parents.

- [ ] **Step 6: Verify parent order and exact tree delta**

Run:

```bash
merge_commit=$(git -C /Users/po1nt9/Github/grok-app-history-integration rev-parse HEAD)
test "$(git -C /Users/po1nt9/Github/grok-app-history-integration show -s --format='%P' "$merge_commit")" = "d2a2563f19bba46cb67496d3b4ac821a31bceaed 05e2aaa96bbdeb68d9740d102ccb1723e26673ed"
test "$(git -C /Users/po1nt9/Github/grok-app-history-integration diff --name-only d2a2563f19bba46cb67496d3b4ac821a31bceaed "$merge_commit" | sort | tr '\n' ' ')" = ".gitignore docs/superpowers/plans/2026-07-28-repository-brand-baseline.md docs/superpowers/specs/2026-07-28-omp-desktop-design.md "
git -C /Users/po1nt9/Github/grok-app-history-integration diff --exit-code d2a2563f19bba46cb67496d3b4ac821a31bceaed "$merge_commit" -- . ':(exclude).gitignore' ':(exclude)docs/superpowers/specs/2026-07-28-omp-desktop-design.md' ':(exclude)docs/superpowers/plans/2026-07-28-repository-brand-baseline.md'
```

Expected: all commands exit `0`.

- [ ] **Step 7: Move `main` to the verified merge without force-cleaning ignored files**

Run:

```bash
merge_commit=$(git -C /Users/po1nt9/Github/grok-app-history-integration rev-parse HEAD)
git branch main "$merge_commit"
git clean -nd
```

Review the dry-run output against the 597-file upstream set. Then run:

```bash
git clean -fd
git switch main
git worktree remove /Users/po1nt9/Github/grok-app-history-integration
git branch -d integrate/omp-upstream-history
```

Expected: `.superpowers/` remains because `-x` was not used; `git status --short --branch` prints a clean `main`.

- [ ] **Step 8: Record the unmodified upstream build baseline**

Run before any product changes:

```bash
set -o pipefail
pnpm install --frozen-lockfile
pnpm typecheck 2>&1 | tee /tmp/omp-plan1-upstream-typecheck.txt
pnpm test 2>&1 | tee /tmp/omp-plan1-upstream-tests.txt
pnpm build:ui 2>&1 | tee /tmp/omp-plan1-upstream-build.txt
cargo test --manifest-path src-tauri/Cargo.toml --locked 2>&1 | tee /tmp/omp-plan1-upstream-rust-tests.txt
```

Expected: record the actual exit status and test counts for each command. If an upstream baseline command fails, preserve its complete output and classify it as a pre-existing failure before changing source; do not silently convert the failure into a Plan 1 regression waiver.

- [ ] **Step 9: Record the history integration commit**

No additional commit is needed: the two-parent merge commit is the independently reviewable deliverable. Preserve `docs/omp-desktop-design` until `main` and archive refs have been pushed to an approved origin.

---

### Task 2: Establish Origin, OMP Fork, and the Pinned Runtime Submodule

**Files:**
- Create: `.gitmodules`
- Create: `runtime/oh-my-pi` gitlink

**Interfaces:**
- Consumes: verified `main` from Task 1; GitHub account `Po1nt9`; official OMP commit `667111575ebba136dadfd6989379e7f67e0d40d9`.
- Produces: planned origin metadata, optional published repositories after confirmation, and a pinned OMP Fork submodule with official upstream configured.

- [ ] **Step 1: Verify that publishing targets do not already exist**

Run:

```bash
! gh repo view Po1nt9/omp-desktop >/dev/null 2>&1
! gh repo view Po1nt9/oh-my-pi >/dev/null 2>&1
```

Expected: both negated checks exit `0`. If either repository exists, stop and inspect ownership/default branch before changing remotes.

- [ ] **Step 2: Obtain explicit publication confirmation**

Before any `gh repo create`, `gh repo fork`, or `git push`, ask the user to approve creating:

```text
https://github.com/Po1nt9/omp-desktop
https://github.com/Po1nt9/oh-my-pi
```

If approval is declined, skip Steps 3–4, keep the URLs only in `provenance/upstreams.json` with `publicationState: "planned"`, and continue local implementation through Task 13. Task 14 cannot mark Plan 1 complete until the submodule URL is remotely cloneable and the reviewed refs are durably published; report that publication gate as the sole remaining blocker rather than weakening provenance checks.

- [ ] **Step 3: Create the repositories only after approval**

Run:

```bash
gh repo create Po1nt9/omp-desktop --public --description "Open-source desktop Agent application powered by OMP"
gh repo fork can1357/oh-my-pi --clone=false --remote=false --fork-name oh-my-pi
git remote add origin https://github.com/Po1nt9/omp-desktop.git
```

Expected: both `gh repo view` commands succeed and `git remote get-url origin` prints the exact OMP Desktop URL.

- [ ] **Step 4: Push only the reviewed history refs after approval**

Run:

```bash
git push -u origin main
git push origin archive/pre-upstream-design
git push origin archive/pre-upstream-design-20260728 baseline/grok-app-d2a2563
```

Expected: `main` tracks `origin/main`; both tags and the archive branch exist remotely.

- [ ] **Step 5: Add the OMP Fork as a source submodule**

Run:

```bash
git submodule add https://github.com/Po1nt9/oh-my-pi.git runtime/oh-my-pi
git -C runtime/oh-my-pi remote add upstream https://github.com/can1357/oh-my-pi.git
git -C runtime/oh-my-pi fetch upstream --prune --tags
git -C runtime/oh-my-pi checkout --detach 667111575ebba136dadfd6989379e7f67e0d40d9
```

If publication was declined, use the already-present local repository only to prepare a local gitlink:

```bash
git -c protocol.file.allow=always submodule add /Users/po1nt9/Github/oh-my-pi runtime/oh-my-pi
git -C runtime/oh-my-pi remote set-url origin https://github.com/Po1nt9/oh-my-pi.git
git -C runtime/oh-my-pi remote add upstream https://github.com/can1357/oh-my-pi.git
git -C runtime/oh-my-pi checkout --detach 667111575ebba136dadfd6989379e7f67e0d40d9
```

Expected: `git submodule status` begins with `667111575e... runtime/oh-my-pi`; `.gitmodules` records the planned team Fork URL, not the local path.

- [ ] **Step 6: Verify ancestry and cleanliness**

Run:

```bash
test "$(git -C runtime/oh-my-pi rev-parse HEAD)" = "667111575ebba136dadfd6989379e7f67e0d40d9"
git -C runtime/oh-my-pi merge-base --is-ancestor 667111575ebba136dadfd6989379e7f67e0d40d9 upstream/main
test -z "$(git -C runtime/oh-my-pi status --porcelain)"
```

Expected: all commands exit `0`.

- [ ] **Step 7: Commit the pinned source relationship**

Run:

```bash
git add .gitmodules runtime/oh-my-pi
git commit -m "build: pin OMP runtime source"
```

Expected: commit contains one `.gitmodules` file and one gitlink; no OMP source files are copied into the superproject.

---

### Task 3: Add Machine-Readable Upstream Provenance

**Files:**
- Create: `provenance/README.md`
- Create: `provenance/upstreams.json`
- Create: `provenance/omp-patches.json`
- Create: `scripts/check-provenance.mjs`
- Test: `scripts/check-provenance.test.mjs`
- Modify: `package.json`

**Interfaces:**
- Consumes: remotes and submodule from Tasks 1–2.
- Produces: `pnpm check:provenance`; JSON schema version `1`; patch ledger consumed by future OMP sync plans.

- [ ] **Step 1: Write the failing checker test**

Create `scripts/check-provenance.test.mjs`:

```js
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { validateUpstreams } from "./check-provenance.mjs";

test("requires exact Grok and OMP baselines", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "omp-provenance-"));
  await writeFile(path.join(root, "upstreams.json"), JSON.stringify({ schemaVersion: 1 }));
  assert.throws(() => validateUpstreams(root), /grokApp\.importCommit/);
});
```

- [ ] **Step 2: Run the test and verify the missing module failure**

Run:

```bash
node --test scripts/check-provenance.test.mjs
```

Expected: FAIL with `ERR_MODULE_NOT_FOUND` for `check-provenance.mjs`.

- [ ] **Step 3: Create the pinned provenance records**

Create `provenance/upstreams.json`:

```json
{
  "schemaVersion": 1,
  "desktop": {
    "repository": "https://github.com/Po1nt9/omp-desktop.git",
    "publicationState": "planned-or-published"
  },
  "grokApp": {
    "remote": "https://github.com/RongleCat/grok-app.git",
    "importCommit": "d2a2563f19bba46cb67496d3b4ac821a31bceaed",
    "importedAt": "2026-07-28",
    "historyMode": "two-parent-merge"
  },
  "omp": {
    "officialRemote": "https://github.com/can1357/oh-my-pi.git",
    "forkRemote": "https://github.com/Po1nt9/oh-my-pi.git",
    "submodulePath": "runtime/oh-my-pi",
    "pinnedCommit": "667111575ebba136dadfd6989379e7f67e0d40d9",
    "officialBaseCommit": "667111575ebba136dadfd6989379e7f67e0d40d9"
  }
}
```

Create `provenance/omp-patches.json`:

```json
{
  "schemaVersion": 1,
  "baseCommit": "667111575ebba136dadfd6989379e7f67e0d40d9",
  "patches": []
}
```

Document in `provenance/README.md` that `origin` is writable, `grok-app-upstream` is read-only, submodule `origin` is the team Fork, submodule `upstream` is official OMP, and every non-upstream OMP commit requires an entry in `omp-patches.json`.

- [ ] **Step 4: Implement exact record validation**

Create `scripts/check-provenance.mjs`:

```js
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const GROK_BASE = "d2a2563f19bba46cb67496d3b4ac821a31bceaed";
const OMP_BASE = "667111575ebba136dadfd6989379e7f67e0d40d9";

export function validateUpstreams(directory) {
  const file = path.join(directory, "upstreams.json");
  const data = JSON.parse(fs.readFileSync(file, "utf8"));
  if (data.grokApp?.importCommit !== GROK_BASE) throw new Error("grokApp.importCommit must match the frozen baseline");
  if (data.omp?.pinnedCommit !== OMP_BASE) throw new Error("omp.pinnedCommit must match the frozen baseline");
  if (data.omp?.submodulePath !== "runtime/oh-my-pi") throw new Error("omp.submodulePath must be runtime/oh-my-pi");
  return data;
}

export function checkRepository(root) {
  const data = validateUpstreams(path.join(root, "provenance"));
  const head = execFileSync("git", ["-C", path.join(root, data.omp.submodulePath), "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  if (head !== data.omp.pinnedCommit) throw new Error(`submodule HEAD ${head} does not match ${data.omp.pinnedCommit}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) checkRepository(process.cwd());
```

- [ ] **Step 5: Add and run the package command**

Add to `package.json` scripts:

```json
"check:provenance": "node scripts/check-provenance.mjs"
```

Run:

```bash
node --test scripts/check-provenance.test.mjs
pnpm check:provenance
```

Expected: both commands PASS with exit `0`.

- [ ] **Step 6: Commit the provenance baseline**

Run:

```bash
git add provenance scripts/check-provenance.mjs scripts/check-provenance.test.mjs package.json
git commit -m "chore: record upstream provenance"
```

---

### Task 4: Add the Brand and Runtime-Coupling Policy Scanner

**Files:**
- Create: `scripts/brand-policy.mjs`
- Create: `scripts/check-brand-policy.mjs`
- Test: `scripts/check-brand-policy.test.mjs`
- Create: `testdata/brand-policy/allowed/*.json`
- Create: `testdata/brand-policy/denied/*.json`
- Modify: `package.json`

**Interfaces:**
- Consumes: repository files after history import.
- Produces: `scanText(path, text): Violation[]`, `checkRepository(root): Violation[]`, and `pnpm check:brand`.

- [ ] **Step 1: Add allowed and denied fixtures**

Create `testdata/brand-policy/allowed/provider-xai.json`:

```json
{"provider":{"id":"xai","name":"xAI","endpoint":"https://api.x.ai","authMethods":["xAI OAuth"]}}
```

Create `testdata/brand-policy/allowed/model-grok.json`:

```json
{"models":[{"id":"grok-4.5","displayName":"Grok 4.5","providerId":"xai"}]}
```

Create denied fixtures:

```json
{"productName":"Grok Desktop"}
```

```json
{"desktopAuthEndpoint":"https://auth.x.ai/oauth/token"}
```

```json
{"method":"_x.ai/ask_user_question"}
```

```json
{"message":"Open omp settings"}
```

- [ ] **Step 2: Write failing policy tests**

Create `scripts/check-brand-policy.test.mjs`:

```js
import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { scanText } from "./check-brand-policy.mjs";

const read = (name) => fs.readFileSync(new URL(`../testdata/brand-policy/${name}`, import.meta.url), "utf8");

test("allows structured runtime Provider and model identities", () => {
  assert.deepEqual(scanText("testdata/brand-policy/allowed/provider-xai.json", read("allowed/provider-xai.json")), []);
  assert.deepEqual(scanText("testdata/brand-policy/allowed/model-grok.json", read("allowed/model-grok.json")), []);
});

test("rejects product branding, direct xAI auth, private methods, and lowercase user-facing OMP", () => {
  assert.ok(scanText("testdata/brand-policy/denied/app-title-grok.json", read("denied/app-title-grok.json")).length > 0);
  assert.ok(scanText("testdata/brand-policy/denied/direct-auth-xai.json", read("denied/direct-auth-xai.json")).length > 0);
  assert.ok(scanText("testdata/brand-policy/denied/private-method-xai.json", read("denied/private-method-xai.json")).length > 0);
  assert.ok(scanText("src/i18n/lowercase-brand.json", read("denied/lowercase-brand.json")).length > 0);
});
```

- [ ] **Step 3: Run the test and verify the missing module failure**

Run:

```bash
node --test scripts/check-brand-policy.test.mjs
```

Expected: FAIL with `ERR_MODULE_NOT_FOUND`.

- [ ] **Step 4: Define explicit rules and allowlists**

Create `scripts/brand-policy.mjs`:

```js
export const textExtensions = new Set([".ts", ".tsx", ".js", ".mjs", ".rs", ".json", ".toml", ".xml", ".plist", ".html", ".md", ".yml", ".yaml", ".sh", ".py"]);

export const userVisiblePathPatterns = [
  /^src\/i18n\//,
  /^src\/components\/.*\.(?:ts|tsx)$/,
  /^src-tauri\/src\/(?:tray|tray_i18n|remote_im|mirror)\//,
  /^README(?:_EN|_ZH)?\.md$/,
];

export const rules = [
  ["grok-product-brand", /\bGrok (?:App|Desktop|Build|CLI)\b/gi],
  ["supergrok-brand", /\bSuperGrok(?:\s*Pro|\s*Heavy)?\b/gi],
  ["legacy-identifier", /\b(?:grokapp|grok_app_lib|grok_agent_stdio)\b|com\.grokapp\.desktop/gi],
  ["private-xai-method", /_x\.ai\//g],
  ["legacy-runtime-env", /\bGROK_(?:HOME|BIN|CLI|APP_ACP|APP_HOME|REMOTE_BRIDGE_HOME|CLI_ALLOW_UNVERIFIED)\b/g],
  ["legacy-runtime-path", /(?:~|\$HOME|%USERPROFILE%)?[\\/]\.grok(?:[\\/]|$)/gi],
  ["desktop-direct-xai", /https:\/\/(?:auth|accounts|api)\.x\.ai|https:\/\/(?:cli-chat-proxy|code)\.grok\.com/gi],
];

export const wholeFileAllowlist = new Set([
  "LICENSE",
  "docs/superpowers/specs/2026-07-28-omp-desktop-design.md",
  "docs/upstream-history/grok-app/README.md",
  "THIRD_PARTY_NOTICES",
  "scripts/brand-policy.mjs",
  "scripts/check-brand-policy.test.mjs",
]);

export const structuredAllowlist = new Map([
  ["testdata/brand-policy/allowed/provider-xai.json", new Set(["xai", "xAI", "https://api.x.ai", "xAI OAuth"])],
  ["testdata/brand-policy/allowed/model-grok.json", new Set(["grok-4.5", "Grok 4.5"])],
]);
```

- [ ] **Step 5: Implement deterministic scanning**

Create `scripts/check-brand-policy.mjs`:

```js
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { rules, structuredAllowlist, textExtensions, userVisiblePathPatterns, wholeFileAllowlist } from "./brand-policy.mjs";

const lowercaseBrand = /(?<![A-Za-z0-9_-])(?:omp|Omp|oMp|omP)(?![A-Za-z0-9_-])/g;

export function scanText(file, text) {
  if (wholeFileAllowlist.has(file)) return [];
  const allowed = structuredAllowlist.get(file) ?? new Set();
  const violations = [];
  for (const [rule, pattern] of rules) {
    pattern.lastIndex = 0;
    for (const match of text.matchAll(pattern)) {
      if (!allowed.has(match[0])) violations.push({ file, rule, match: match[0], index: match.index });
    }
  }
  if (userVisiblePathPatterns.some((pattern) => pattern.test(file))) {
    lowercaseBrand.lastIndex = 0;
    for (const match of text.matchAll(lowercaseBrand)) {
      if (!allowed.has(match[0])) violations.push({ file, rule: "lowercase-user-visible-omp", match: match[0], index: match.index });
    }
  }
  return violations;
}

export function checkRepository(root) {
  const files = execFileSync("git", ["-C", root, "ls-files"], { encoding: "utf8" }).trim().split("\n").filter(Boolean);
  return files.flatMap((file) => textExtensions.has(path.extname(file)) ? scanText(file, fs.readFileSync(path.join(root, file), "utf8")) : []);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const violations = checkRepository(process.cwd());
  for (const item of violations) console.error(`${item.file}: ${item.rule}: ${item.match}`);
  if (violations.length) process.exitCode = 1;
}
```

- [ ] **Step 6: Verify fixture behavior, then capture the initial repository failure**

Run:

```bash
node --test scripts/check-brand-policy.test.mjs
node scripts/check-brand-policy.mjs > /tmp/omp-brand-baseline.txt 2>&1; test $? -eq 1
```

Expected: fixture tests PASS; repository scan FAILS and `/tmp/omp-brand-baseline.txt` lists current Grok App violations. Do not add broad exceptions to make this pass.

- [ ] **Step 7: Register the scanner and commit**

Add to `package.json`:

```json
"check:brand": "node scripts/check-brand-policy.mjs"
```

Run:

```bash
git add package.json scripts/brand-policy.mjs scripts/check-brand-policy.mjs scripts/check-brand-policy.test.mjs testdata/brand-policy
git commit -m "test: enforce OMP brand boundaries"
```

Expected: the commit succeeds even though full-repository `pnpm check:brand` remains red; Tasks 5–13 reduce the known baseline to zero.

### Task 5: Rename Product Metadata and Replace Brand Assets

**Files:**
- Modify: `package.json`, `pnpm-lock.yaml`, `index.html`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/tauri.macos.conf.json`, `src-tauri/tauri.windows.conf.json`, `src-tauri/Info.plist`
- Create: `src-tauri/icons/omp-mark.svg`
- Modify: `scripts/generate-icons.sh`
- Replace: `public/logo.png`, `assets/logo.png`, `src-tauri/icons/*`
- Test: `src/lib/window-config.test.ts`

**Interfaces:**
- Consumes: brand scanner from Task 4.
- Produces: package `omp-desktop`, crate `omp-desktop`, Rust library `omp_desktop_lib`, product `OMP Desktop`, identifier `io.github.po1nt9.omp-desktop`, and approved asset hashes.

- [ ] **Step 1: Change the manifest test first**

Update `src/lib/window-config.test.ts` to assert:

```ts
expect(config.productName).toBe("OMP Desktop");
expect(config.identifier).toBe("io.github.po1nt9.omp-desktop");
expect(config.app.windows[0]?.title).toBe("OMP Desktop");
```

Run:

```bash
pnpm test -- src/lib/window-config.test.ts
```

Expected: FAIL because manifests still contain Grok metadata.

- [ ] **Step 2: Update npm, Cargo, and Tauri metadata atomically**

Apply these exact values:

```json
{
  "name": "omp-desktop",
  "version": "0.1.9",
  "description": "Open-source desktop Agent application powered by OMP",
  "license": "MIT",
  "repository": {"type":"git","url":"git+https://github.com/Po1nt9/omp-desktop.git"},
  "homepage": "https://github.com/Po1nt9/omp-desktop#readme",
  "bugs": {"url":"https://github.com/Po1nt9/omp-desktop/issues"}
}
```

Use this Cargo package metadata:

```toml
[package]
name = "omp-desktop"
version = "0.1.9"
description = "Open-source desktop Agent application powered by OMP"
authors = ["Po1nt9", "RongleCat"]
license = "MIT"
repository = "https://github.com/Po1nt9/omp-desktop"
homepage = "https://github.com/Po1nt9/omp-desktop"

[lib]
name = "omp_desktop_lib"
```

Set Tauri `productName`, every window title, and platform overlay title to `OMP Desktop`; set `identifier` to `io.github.po1nt9.omp-desktop`; keep version `0.1.9`. Remove `$HOME/.grok/auth.json` from filesystem scope and remove hardcoded xAI/Grok domains from CSP.

Run:

```bash
pnpm install --lockfile-only
cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1 --locked >/dev/null
```

Expected: lockfiles update only for the package/crate rename; metadata exits `0`.

- [ ] **Step 3: Add an original deterministic OMP master mark**

Create `src-tauri/icons/omp-mark.svg` with this original geometric artwork:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-labelledby="title">
  <title id="title">OMP</title>
  <rect width="512" height="512" rx="112" fill="#111318"/>
  <path d="M104 152h112v208H104zM136 184v144h48V184z" fill="#f3f4f6" fill-rule="evenodd"/>
  <path d="M232 152h48l40 76 40-76h48v208h-48V240l-40 72-40-72v120h-48z" fill="#f06a3c"/>
  <path d="M424 152h-48v208h48V248h24c40 0 64-18 64-48s-24-48-64-48zm0 40h22c12 0 18 3 18 8s-6 8-18 8h-22z" transform="translate(-24)" fill="#f3f4f6"/>
</svg>
```

Update `scripts/generate-icons.sh` so its only source is `src-tauri/icons/omp-mark.svg` and its generated outputs are the existing PNG/ICO/ICNS/tray paths. Delete `docs/svg/SuperGrok.svg` and `docs/svg/SuperGrokHeavy.svg`.

- [ ] **Step 4: Generate assets and record their hashes**

Run:

```bash
bash scripts/generate-icons.sh
shasum -a 256 src-tauri/icons/omp-mark.svg public/logo.png src-tauri/icons/icon.icns src-tauri/icons/icon.ico > src-tauri/icons/ASSET_HASHES.txt
```

Expected: every output exists and is non-empty. Open the SVG, PNG, ICO, ICNS, and tray preview and record approval in the commit message body; automated hashes do not replace visual review.

- [ ] **Step 5: Update frontend logo components without compatibility aliases**

Rename `src/components/GrokLogo.tsx` to `src/components/OmpLogo.tsx`; export `OmpLogo`; rename `IconGrokMark` to `IconOmpMark`; replace `.grok-logo` with `.omp-logo`. Delete `src/components/SuperGrokMark.tsx` and all imports. Use `aria-label="OMP"` and visible `OMP`, never lowercase branding.

- [ ] **Step 6: Run focused metadata and build validation**

Run:

```bash
pnpm test -- src/lib/window-config.test.ts
pnpm typecheck
pnpm build:ui
```

Expected: all PASS.

- [ ] **Step 7: Commit metadata and original assets**

Run:

```bash
git add package.json pnpm-lock.yaml index.html src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri*.json src-tauri/Info.plist src-tauri/icons public/logo.png assets/logo.png scripts/generate-icons.sh src/components src/styles/app.css src/lib/window-config.test.ts
git commit -m "feat: establish OMP Desktop identity"
```

---

### Task 6: Remove Grok CLI, Account, Quota, and Direct Credential Modules

**Files:**
- Delete: `src-tauri/src/cli_probe.rs`, `cli_install.rs`, `cli_update.rs`, `cli_sessions.rs`
- Delete: `src-tauri/src/account.rs`, `account_profiles.rs`, `supergrok_quota.rs`, `voice_auth.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`
- Modify: `src/lib/api.ts`
- Delete or split tests: `src/lib/cliDoctor.test.ts`, `managedSetup.test.ts`, `accountUi.test.ts`

**Interfaces:**
- Consumes: renamed Host package from Task 5.
- Produces: no command registration or TypeScript invoke wrapper for Grok CLI/account/quota; generic initials and URL-opening helpers remain in neutral modules.

- [ ] **Step 1: Add a command-surface regression test**

In the existing Rust command registration test module, add:

```rust
#[test]
fn command_surface_has_no_grok_product_commands() {
    let commands = registered_command_names();
    for removed in [
        "probe_cli", "install_cli_latest", "check_cli_update", "install_cli_update",
        "account_status", "account_login", "account_logout", "supergrok_quota",
    ] {
        assert!(!commands.contains(&removed), "legacy command remained: {removed}");
    }
}
```

If command names are currently embedded directly in `generate_handler!`, first extract a test-only `registered_command_names() -> &'static [&'static str]` adjacent to the handler list, populated from the same named constants.

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml command_surface_has_no_grok_product_commands --locked
```

Expected: FAIL and identify the first still-registered command.

- [ ] **Step 2: Remove module declarations and Tauri command registrations**

Delete the four CLI modules and four account/quota/auth modules. Remove their `mod` declarations, imports, managed state, `generate_handler!` entries, and command forwarding functions from `src-tauri/src/lib.rs` and `commands.rs`. Preserve generic `open_external_url` by moving it to the existing desktop-shell helper module; do not preserve billing or subscription URLs.

- [ ] **Step 3: Remove matching frontend APIs and DTOs**

Delete TypeScript invoke wrappers and DTOs for CLI install/probe/update/session import, account login/logout/status, saved auth profiles, quota, subscription, and speech access tokens from `src/lib/api.ts`. Do not replace them with OMP stubs.

- [ ] **Step 4: Preserve only generic account UI helpers**

Move pure initials/display-name helpers from `src/lib/accountUi.ts` to `src/lib/displayIdentity.ts`:

```ts
export function identityInitials(label: string): string {
  return label.trim().split(/\s+/u).filter(Boolean).slice(0, 2).map((part) => part[0] ?? "").join("").toUpperCase();
}
```

Add `src/lib/displayIdentity.test.ts` with empty, single-word, and two-word cases. Delete SuperGrok channel/cache/quota logic and its tests.

- [ ] **Step 5: Verify removal and preserved helper behavior**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml command_surface_has_no_grok_product_commands --locked
pnpm test -- src/lib/displayIdentity.test.ts
! git grep -nE 'install_cli_latest|supergrok_quota|account_login|speech_access_token' -- ':!docs/upstream-history/**' ':!docs/superpowers/specs/**'
```

Expected: tests PASS and grep exits `0` because no production references remain.

- [ ] **Step 6: Commit the removed product backends**

Run:

```bash
git add -A src-tauri/src src/lib
git commit -m "refactor: remove Grok account and CLI backends"
```

---

### Task 7: Retain Standard ACP Framing and Make Sessions Fail Closed

**Files:**
- Modify: `src-tauri/src/acp_client.rs`, `session_manager.rs`, `commands.rs`
- Create: `src-tauri/src/runtime_availability.rs`
- Create: `src/lib/runtimeAvailability.ts`
- Test: `src/lib/runtimeAvailability.test.ts`
- Modify/delete: `src-tauri/tests/fixtures/acp/*`, `src-tauri/src/acp_golden_test.rs`

**Interfaces:**
- Consumes: command cleanup from Task 6.
- Produces: stable `runtime_unavailable` error; standard ACP framing/parser remains; no process spawn or `_x.ai/*` binding remains.

- [ ] **Step 1: Define the frontend fail-closed contract with a failing test**

Create `src/lib/runtimeAvailability.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { runtimeAvailability } from "./runtimeAvailability";

describe("runtimeAvailability", () => {
  it("is unavailable until a later plan connects OMP", () => {
    expect(runtimeAvailability).toEqual({ available: false, reason: "runtime_unavailable" });
  });
});
```

Run `pnpm test -- src/lib/runtimeAvailability.test.ts`.

Expected: FAIL because the module is missing.

- [ ] **Step 2: Implement the shared stable state**

Create `src/lib/runtimeAvailability.ts`:

```ts
export type RuntimeUnavailableReason = "runtime_unavailable";
export const runtimeAvailability = { available: false, reason: "runtime_unavailable" } as const;
```

Create `src-tauri/src/runtime_availability.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAvailability {
    pub available: bool,
    pub reason: &'static str,
}

pub const RUNTIME_AVAILABILITY: RuntimeAvailability = RuntimeAvailability {
    available: false,
    reason: "runtime_unavailable",
};
```

Expose a read-only Tauri command returning this constant.

- [ ] **Step 3: Remove Grok spawn behavior but keep transport code**

In `acp_client.rs`, delete `GROK_APP_ACP`, `GROK_HOME`, Grok CLI arguments, sandbox/reasoning/leader/preferred-agent flags, and direct spawning. Retain standard JSON-RPC framing, pending-request management, standard ACP initialize/session/prompt/cancel/update/request-permission types, and protocol decode tests. Make any Plan 1 call that would spawn return a typed Host error with code `runtime_unavailable`.

- [ ] **Step 4: Remove private method production bindings and fixtures**

Delete `_x.ai/session/update`, `_x.ai/session/prompt_complete`, `_x.ai/interject`, `_x.ai/exit_plan_mode`, and `_x.ai/ask_user_question` send/decode paths. Delete `ask_user_question.json` and `exit_plan_mode.json`; remove private completion entries from `stream_chunks.json`; keep standard ACP fixtures. Do not introduce `_omp` strings.

- [ ] **Step 5: Replace backend IDs without claiming OMP connectivity**

Replace product state `grok_agent_stdio` with `runtime_unavailable`. Session creation, resume, prompt, cancel, interject, plan approval, and elicitation commands must return the same stable unavailable error before touching a process. Preserve journal/list/search UI that does not execute an Agent.

- [ ] **Step 6: Verify standard ACP and unavailable-state behavior**

Run:

```bash
pnpm test -- src/lib/runtimeAvailability.test.ts
cargo test --manifest-path src-tauri/Cargo.toml acp_golden --locked
cargo test --manifest-path src-tauri/Cargo.toml runtime_availability --locked
! git grep -n '_x\.ai/' -- src src-tauri ':!src-tauri/tests/fixtures/acp/README.md'
! git grep -nE 'GROK_APP_ACP|grok_agent_stdio' -- src src-tauri
```

Expected: all tests PASS; both greps exit `0`.

- [ ] **Step 7: Commit the fail-closed ACP shell**

Run:

```bash
git add -A src-tauri/src/acp_client.rs src-tauri/src/session_manager.rs src-tauri/src/commands.rs src-tauri/src/runtime_availability.rs src-tauri/tests/fixtures/acp src-tauri/src/acp_golden_test.rs src/lib/runtimeAvailability.ts src/lib/runtimeAvailability.test.ts
git commit -m "refactor: make Agent runtime fail closed"
```

---

### Task 8: Remove Grok Configuration, Catalog, and Direct xAI Voice Coupling

**Files:**
- Modify: `src-tauri/src/paths.rs`, `providers.rs`, `models_catalog.rs`, `extensions.rs`, `hooks.rs`, `project_rules.rs`, `agent_memory.rs`, `agent_subagents.rs`, `permission_rules.rs`
- Modify/delete: `src-tauri/src/voice_stt.rs`, `voice_host.rs`
- Delete: `src-tauri/src/cc_switch_import.rs`, `scripts/probe-models.sh`
- Modify: `src/lib/grokCatalog.ts` (rename to `src/lib/modelOptions.ts`)
- Test: `src/lib/modelOptions.test.ts`

**Interfaces:**
- Consumes: `runtime_unavailable` contract from Task 7.
- Produces: reusable model/effort types with an empty runtime catalog; no direct `.grok`, `config.toml`, xAI token, or xAI WebSocket access.

- [ ] **Step 1: Rename and narrow model option tests first**

Rename `src/lib/grokCatalog.test.ts` to `src/lib/modelOptions.test.ts`. Replace hardcoded model expectations with:

```ts
import { describe, expect, it } from "vitest";
import { availableModels, effortDisplayLabel } from "./modelOptions";

describe("model options before runtime integration", () => {
  it("does not invent a fallback model", () => expect(availableModels).toEqual([]));
  it("keeps neutral effort labels", () => expect(effortDisplayLabel("high")).toBe("High"));
});
```

Run `pnpm test -- src/lib/modelOptions.test.ts`.

Expected: FAIL because the renamed neutral module does not exist.

- [ ] **Step 2: Keep neutral types and remove the static Grok catalog**

Rename `grokCatalog.ts` to `modelOptions.ts`; retain `ModelOption`, `EffortOption`, `effortsForModel`, `pickDefaultEffort`, and `effortDisplayLabel`. Export:

```ts
export const availableModels: readonly ModelOption[] = [];
export const defaultModelId: string | null = null;
```

Delete `GROK_BUILD_MODELS`, `GROK_BUILD_EFFORTS`, and every `grok-4.5` fallback.

- [ ] **Step 3: Separate Desktop data paths from runtime discovery**

In `paths.rs`, retain OMP Desktop app data, projects, UI sessions, logs, attachments, and workspaces under the new application namespace. Delete `resolve_agent_grok_home`, `agent_home_dir`, `.grok` session-layout assumptions, and Grok auth/config resolution. Runtime-owned feature commands return `runtime_unavailable` rather than reading files directly.

- [ ] **Step 4: Disable runtime-owned configuration writers**

Remove Grok `config.toml` writes, Grok Provider routes, official/SuperGrok account assumptions, CC Switch import, `grok inspect`, `grok mcp`, extension/plugin CLI calls, and direct `.grok` scanning. Preserve pure parsers, DTOs, form validation, and UI projection helpers; gate every read/write command through the unavailable state.

- [ ] **Step 5: Remove direct xAI voice network/auth code**

Delete token extraction and direct xAI STT/realtime WebSocket endpoints. Preserve local audio capture and waveform UI only if they compile without any network credential path; otherwise hide the voice action behind `runtime_unavailable` and retain no direct network code.

- [ ] **Step 6: Verify the neutral catalog and absence of direct coupling**

Run:

```bash
pnpm test -- src/lib/modelOptions.test.ts
! git grep -nE 'GROK_HOME|~/\.grok|grok-4\.5|api\.x\.ai|auth\.x\.ai|accounts\.x\.ai|cli-chat-proxy\.grok\.com' -- src src-tauri scripts ':!testdata/brand-policy/allowed/**'
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Expected: test suites PASS; grep exits `0`.

- [ ] **Step 7: Commit the neutral configuration shell**

Run:

```bash
git add -A src-tauri/src src/lib scripts/probe-models.sh
git commit -m "refactor: remove Grok runtime configuration coupling"
```

---

### Task 9: Remove Grok Account and Setup Surfaces from the Frontend

**Files:**
- Modify: `src/App.tsx`, `src/lib/api.ts`, `src/lib/settingsCatalog.ts`
- Modify: `src/components/UserMenu.tsx`, `SettingsPage.tsx`, `SetupWizard.tsx`, `DoctorModal.tsx`, `ProvidersPanel.tsx`, `ComposerModelMenu.tsx`
- Delete: `src/components/CliUpdateRow.tsx`, `ManagedSetupPanel.tsx`, `SuperGrokMark.tsx`
- Modify: frontend tests covering settings, setup, models, errors, and sessions.

**Interfaces:**
- Consumes: `runtimeAvailability` and empty model catalog from Tasks 7–8.
- Produces: navigable OMP Desktop UI with read-only local workspace/history features and no callable Agent/account/provider action.

- [ ] **Step 1: Add a fail-closed interaction test**

In the existing setup/workbench component test suite, add:

```tsx
it("disables Agent execution until OMP Runtime integration exists", async () => {
  render(<App />);
  expect(screen.getByRole("button", { name: /send/i })).toBeDisabled();
  expect(screen.getByText(/OMP Runtime is not connected/i)).toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalledWith(expect.stringMatching(/prompt|login|provider/i), expect.anything());
});
```

Run the focused suite.

Expected: FAIL because legacy setup/account/runtime actions remain reachable.

- [ ] **Step 2: Remove account, quota, subscription, and saved-profile state from `App.tsx`**

Delete state/effects/routes for account status, login/logout, quota, SuperGrok, subscription, official Provider account, CLI update/setup/import, and Grok session import. Keep project selection, UI session list, resource preview, local settings, tray, updater, and remote channel configuration.

- [ ] **Step 3: Make execution affordances consume one availability object**

Use:

```tsx
const runtimeBlocked = !runtimeAvailability.available;
const runtimeBlockedMessage = t("runtime.unavailable.plan1");
```

Disable send, resume, cancel, model selection, Provider credential changes, MCP/Skill writes, Agent preference writes, and remote Agent execution. The visible English source message is `OMP Runtime is not connected in this build.` Equivalent Chinese messages are added in Task 11 without restructuring i18n.

- [ ] **Step 4: Simplify setup and settings without pretending integration exists**

`SetupWizard` must introduce OMP Desktop, allow UI locale/theme/data-directory review, and finish into the fail-closed workspace. Remove Grok CLI install/login/account steps. `DoctorModal` keeps Desktop environment checks but removes Grok CLI checks. `ProvidersPanel` and model menu render an unavailable empty state; they do not display fake models or credentials.

- [ ] **Step 5: Remove dead wrappers and components**

Delete matching `src/lib/api.ts` wrappers, settings search catalog entries, components, hooks, and tests that only exercised deleted Grok functionality. Preserve generic account-free identity rendering through `displayIdentity.ts`.

- [ ] **Step 6: Run focused and full frontend checks**

Run:

```bash
pnpm typecheck
pnpm test
pnpm build:ui
```

Expected: all PASS; no skipped failure is added to compensate for removed functionality.

- [ ] **Step 7: Commit the fail-closed product UI**

Run:

```bash
git add -A src
 git commit -m "refactor: remove Grok product surfaces"
```

### Task 10: Disconnect Remote Agent Execution and Remove the Deprecated Node Bridge

**Files:**
- Delete: `src-tauri/src/remote_im/grok_agent.rs`
- Modify: `src-tauri/src/remote_im/mod.rs`, `engine.rs`, `control_plane.rs`
- Delete: `remote-bridge/`
- Modify: root build scripts/workflows that reference `remote-bridge`
- Test: `src-tauri/src/remote_im/protocol_start_tests.rs` and control-plane tests.

**Interfaces:**
- Consumes: Host `runtime_unavailable` from Task 7.
- Produces: platform adapters and configuration remain testable; every remote request to start/resume an Agent returns `runtime_unavailable`; deprecated Node package is absent from the dependency/build graph.

- [ ] **Step 1: Add a remote execution denial test**

Add to the Remote IM control-plane test module:

```rust
#[tokio::test]
async fn remote_agent_execution_fails_closed_without_runtime() {
    let response = handle_agent_command(test_context(), "/new inspect this repository").await;
    assert_eq!(response.error_code(), Some("runtime_unavailable"));
    assert!(!response.started_process());
}
```

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml remote_agent_execution_fails_closed_without_runtime --locked
```

Expected: FAIL because `grok_agent` still provides a runner.

- [ ] **Step 2: Remove the direct Grok remote runner**

Delete `grok_agent.rs`, its module declaration, binary resolution, resume flags, `GROK_HOME`, and `run_turn` calls. Route Agent commands to the same typed unavailable response as the desktop command surface. Preserve platform connection health, ACL, inbound normalization, outbound text/attachment code, and channel configuration.

- [ ] **Step 3: Prove the deprecated Node bridge is outside the active graph**

Run before deletion:

```bash
git grep -n 'remote-bridge' -- package.json pnpm-lock.yaml .github scripts src-tauri src || true
```

Remove every active build/workflow/package reference, then delete `remote-bridge/` entirely. Do not rename it into an OMP bridge.

- [ ] **Step 4: Verify remote channels remain testable but cannot execute Agents**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml remote_im --locked
! test -e remote-bridge
! git grep -nE 'resolve_grok_binary|GROK_REMOTE_BRIDGE_HOME|grok_agent::run_turn' -- src-tauri src .github scripts
```

Expected: tests PASS; filesystem and grep checks exit `0`.

- [ ] **Step 5: Commit the remote boundary cleanup**

Run:

```bash
git add -A remote-bridge src-tauri/src/remote_im package.json pnpm-lock.yaml .github scripts
git commit -m "refactor: disconnect legacy remote Agent runner"
```

---

### Task 11: Update User-Visible Text, Tray, Updater, and Release Metadata

**Files:**
- Modify: `src/i18n/messages.ts`, `src/i18n/zh-tw.ts`
- Modify: `src-tauri/src/tray.rs`, `tray_i18n.rs`, `lib.rs`, `mirror/http.rs`
- Modify: `src-tauri/src/remote_im/control_plane.rs`, `slash.rs`, channel User-Agents/titles
- Modify: `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- Modify: `scripts/build-release-config.mjs`, `verify-updater-setup.sh`, `release-tag.sh`, `assemble-updater-manifest.sh`, `generate-latest-json.sh`, `changelog-for-release.py`, `package-windows-portable.sh`
- Modify: `src-tauri/src/app_update.rs`, `src/hooks/useUpdater.ts`

**Interfaces:**
- Consumes: deleted product features and renamed metadata from Tasks 5–10.
- Produces: existing three-locale dictionaries and all native surfaces consistently say OMP; updater/release identifiers use OMP technical names.

- [ ] **Step 1: Add focused tray/native-copy assertions**

Update tray i18n tests to assert exact values:

```rust
assert_eq!(catalog.en.open_app, "Open OMP Desktop");
assert_eq!(catalog.en.quit, "Quit OMP Desktop");
assert_eq!(catalog.en.tooltip, "OMP Desktop");
```

Update notification tests to expect `OMP` as the turn-complete title. Run focused Rust tests and confirm they FAIL on old copy.

- [ ] **Step 2: Replace product copy in all three existing locale catalogs**

Change visible `Grok` assistant/app/tray/notification/setup text to `OMP` or `OMP Desktop`. Add the existing-key equivalent of:

```text
en: OMP Runtime is not connected in this build.
zh-CN: 此版本尚未连接 OMP Runtime。
zh-TW: 此版本尚未連接 OMP Runtime。
```

Delete unreachable CLI/account/quota/SuperGrok keys together with their deleted UI references. Do not restructure catalogs; the complete i18n redesign belongs to Plan 6.

- [ ] **Step 3: Rename native IDs and User-Agents**

Use stable technical identifiers:

```rust
const TRAY_ID: &str = "omp-desktop-main-tray";
const USER_AGENT: &str = "OMP-Desktop/0.1.9";
```

Change native titles, mirror title, remote help/card titles, default channel titles, macOS quit label, and all user-visible error summaries to OMP. Rename `GROK_UPDATER_PUBLIC_KEY` and `GROK_UPDATER_ENDPOINT` to `OMP_DESKTOP_UPDATER_PUBLIC_KEY` and `OMP_DESKTOP_UPDATER_ENDPOINT` consistently in Rust, build scripts, documentation, and workflows.

- [ ] **Step 4: Rename release assets and repositories without publishing**

Use `OMP-Desktop` for release display names and `omp-desktop` for technical asset prefixes. Set repository links to `Po1nt9/omp-desktop`. Update Windows executable filters from `grok_app_lib-*.exe` to `omp_desktop_lib-*.exe`. Remove all claims that a separate Grok Build CLI is required.

- [ ] **Step 5: Verify native and release surfaces**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml tray --locked
pnpm typecheck
pnpm test
node scripts/build-release-config.mjs --help >/dev/null 2>&1 || test $? -ne 127
```

Expected: tests PASS; the script exists and executes rather than failing as a missing command.

- [ ] **Step 6: Commit user-visible and release branding**

Run:

```bash
git add src/i18n src-tauri/src .github scripts src/hooks/useUpdater.ts
git commit -m "chore: rename desktop and release surfaces"
```

---

### Task 12: Rewrite Current Documentation and Archive Upstream History

**Files:**
- Modify: `README.md`, `README_EN.md`, `README_ZH.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`
- Modify: `.github/PULL_REQUEST_TEMPLATE.md`, `.github/ISSUE_TEMPLATE/*`
- Create: `docs/upstream-history/grok-app/README.md`
- Move: obsolete Grok-era product plans/reports/acceptance captures under `docs/upstream-history/grok-app/`
- Preserve: `docs/superpowers/specs/2026-07-28-omp-desktop-design.md`

**Interfaces:**
- Consumes: actual fail-closed behavior and package names from Tasks 5–11.
- Produces: current docs that describe only implemented Plan 1 behavior; immutable upstream-history area with a precise brand-policy exception.

- [ ] **Step 1: Add a documentation truth check to the brand-policy test**

Add assertions that current README files contain `OMP Desktop`, contain `runtime_unavailable` or the human-readable unavailable statement, and do not claim users can run Agent prompts, configure Providers, or install a Grok CLI in the Plan 1 build.

Run:

```bash
node --test scripts/check-brand-policy.test.mjs
```

Expected: FAIL on current README content.

- [ ] **Step 2: Rewrite current README files from current behavior**

Each README must state:

```text
OMP Desktop is an open-source Tauri/React desktop shell being adapted to the OMP Runtime.
The Plan 1 baseline is intentionally fail-closed: Agent execution, Provider authentication,
and runtime-owned configuration are unavailable until the versioned OMP integration lands.
```

Document supported development commands, three target operating systems, MIT license, exact Grok App source baseline, exact OMP submodule baseline, and links to the frozen master design. Do not advertise unimplemented Agent capabilities.

- [ ] **Step 3: Update governance and issue templates**

Change repository URLs, product names, security contact instructions, reproduction fields, and contribution setup to OMP Desktop. Preserve RongleCat attribution in LICENSE/notices rather than presenting upstream authorship as new project authorship.

- [ ] **Step 4: Archive historical material without rewriting history**

Move obsolete Grok-era plans, reports, acceptance HTML/PNG, and old architecture notes to `docs/upstream-history/grok-app/`, retaining their contents. Create the directory README:

```md
# Grok App Upstream History

These files preserve historical material imported from RongleCat/grok-app at
`d2a2563f19bba46cb67496d3b4ac821a31bceaed`. They do not describe the current
OMP Desktop product, runtime, security model, or supported features.
```

Add only this directory as a path-scoped `upstream-history` category in `brand-policy.mjs`. Do not allow `docs/**` globally.

- [ ] **Step 5: Verify current docs and archive isolation**

Run:

```bash
node --test scripts/check-brand-policy.test.mjs
pnpm check:brand
```

Expected at this stage: documentation tests PASS; any remaining full-scan violations are limited to legal/provenance inputs addressed in Task 13, not current product docs.

- [ ] **Step 6: Commit documentation truth and history archive**

Run:

```bash
git add -A README.md README_EN.md README_ZH.md CONTRIBUTING.md SECURITY.md CODE_OF_CONDUCT.md .github docs
 git commit -m "docs: establish OMP Desktop project documentation"
```

---

### Task 13: Establish License, Notice, and SBOM Input Baselines

**Files:**
- Create: `third-party/inventory.json`
- Create: `third-party/policy.toml`
- Create: `sbom/inputs.json`
- Create: `THIRD_PARTY_NOTICES`
- Create: `scripts/check-legal-baseline.mjs`
- Test: `scripts/check-legal-baseline.test.mjs`
- Modify: `package.json`

**Interfaces:**
- Consumes: pinned source graph from Tasks 1–3 and deleted bridge from Task 10.
- Produces: `pnpm check:legal`; source-level inventory for Plan 9's final artifact SBOM.

- [ ] **Step 1: Write failing legal baseline tests**

Create `scripts/check-legal-baseline.test.mjs`:

```js
import assert from "node:assert/strict";
import test from "node:test";
import { requiredLegalInputs, validateInventory } from "./check-legal-baseline.mjs";

test("tracks non-MIT OMP resources and upstream notices", () => {
  const paths = new Set(requiredLegalInputs().map((item) => item.path));
  assert.ok(paths.has("runtime/oh-my-pi/crates/pi-natives/src/fonts/Silver.LICENSE"));
  assert.ok(paths.has("runtime/oh-my-pi/packages/coding-agent/src/export/html/vendor/highlight.min.js"));
  assert.equal(validateInventory().length, 0);
});
```

Run:

```bash
node --test scripts/check-legal-baseline.test.mjs
```

Expected: FAIL because the checker is absent.

- [ ] **Step 2: Create a source-level inventory with explicit component ownership**

`third-party/inventory.json` must contain entries for:

```json
[
  {"name":"RongleCat/grok-app","component":"desktop-ui","license":"MIT","sourceCommit":"d2a2563f19bba46cb67496d3b4ac821a31bceaed","licensePath":"LICENSE","bundled":true},
  {"name":"oh-my-pi","component":"omp-sidecar-source","license":"MIT","sourceCommit":"667111575ebba136dadfd6989379e7f67e0d40d9","licensePath":"runtime/oh-my-pi/LICENSE","bundled":false},
  {"name":"Silver","component":"omp-sidecar-resource","license":"CC-BY-4.0","licensePath":"runtime/oh-my-pi/crates/pi-natives/src/fonts/Silver.LICENSE","bundled":false},
  {"name":"highlight.js","component":"omp-sidecar-resource","license":"BSD-3-Clause","licensePath":"runtime/oh-my-pi/packages/coding-agent/src/export/html/vendor/highlight.min.js","bundled":false}
]
```

Also enumerate all tracked OMP `NOTICE` and `crates/vendor/**/LICENSE` paths with their source component. `bundled:false` means Plan 1 does not yet ship a sidecar; Plan 9 must recalculate against artifacts.

- [ ] **Step 3: Define license policy and SBOM inputs**

`third-party/policy.toml` permits MIT, BSD-2-Clause, BSD-3-Clause, Apache-2.0, ISC, Unicode-3.0, and CC-BY-4.0; CC-BY-4.0 is limited to the Silver font entry. Unknown and unapproved copyleft expressions fail.

`sbom/inputs.json` records the root `package.json`/`pnpm-lock.yaml`, Tauri `Cargo.toml`/`Cargo.lock`, submodule `package.json`/`bun.lock`/`Cargo.toml`/`Cargo.lock`/`rust-toolchain.toml`, workspace manifests, legal files, native resources, and vendored sources. It records `remote-bridge` as:

```json
{"path":"remote-bridge","state":"removed","includedInReleaseGraph":false}
```

- [ ] **Step 4: Assemble `THIRD_PARTY_NOTICES`**

Include complete, attributed sections for RongleCat's MIT license; OMP/Pi MIT copyright for Mario Zechner and Can Bölük; the three OMP notices at `crates/pi-shell/NOTICE`, `packages/coding-agent/src/markit/NOTICE`, and `packages/utils/src/vendor/mermaid-ascii/NOTICE`; Silver CC BY 4.0 attribution and URL; full BSD-3-Clause text for highlight.js; and an index of vendored license paths.

- [ ] **Step 5: Implement the legal input checker**

Create `scripts/check-legal-baseline.mjs` that exports `requiredLegalInputs()` and `validateInventory()`, verifies every path exists, verifies source commits equal the provenance records, rejects unknown license expressions, ensures `remote-bridge` is absent, and ensures every tracked `runtime/oh-my-pi/**/{LICENSE,NOTICE}` file appears in the inventory.

- [ ] **Step 6: Run and register the legal gate**

Add:

```json
"check:legal": "node scripts/check-legal-baseline.mjs"
```

Run:

```bash
node --test scripts/check-legal-baseline.test.mjs
pnpm check:legal
```

Expected: both PASS.

- [ ] **Step 7: Commit legal and SBOM inputs**

Run:

```bash
git add THIRD_PARTY_NOTICES third-party sbom scripts/check-legal-baseline.mjs scripts/check-legal-baseline.test.mjs package.json
git commit -m "docs: establish third-party legal baseline"
```

---

### Task 14: Close the Plan 1 Baseline with Full Verification

**Files:**
- Modify only if a gate exposes a Plan 1 regression; do not weaken a gate to make it pass.
- Create: `docs/superpowers/verification/2026-07-28-plan-1-baseline.md`

**Interfaces:**
- Consumes: all prior Plan 1 deliverables.
- Produces: a clean, buildable, fail-closed OMP Desktop baseline and a reproducible verification record.

- [ ] **Step 1: Reinitialize dependencies from lockfiles**

Run:

```bash
pnpm install --frozen-lockfile
git submodule update --init --recursive
test "$(git -C runtime/oh-my-pi rev-parse HEAD)" = "667111575ebba136dadfd6989379e7f67e0d40d9"
```

Expected: install succeeds without lockfile mutation; submodule commit matches exactly.

- [ ] **Step 2: Run all custom policy gates**

Run:

```bash
pnpm check:provenance
pnpm check:brand
pnpm check:legal
node --test scripts/check-provenance.test.mjs scripts/check-brand-policy.test.mjs scripts/check-legal-baseline.test.mjs
```

Expected: all commands exit `0`; brand violations count is zero; legal/provenance checks report no mismatch.

- [ ] **Step 3: Run complete frontend verification**

Run:

```bash
pnpm typecheck
pnpm test
pnpm build:ui
```

Expected: TypeScript reports zero errors, all tests pass, and Vite produces `dist/` successfully.

- [ ] **Step 4: Run complete Rust verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1 >/dev/null
```

Expected: all Rust tests pass and metadata exits `0`.

- [ ] **Step 5: Prove removed runtime paths and preserved Provider/model fixtures**

Run:

```bash
! git grep -nE '_x\.ai/|GROK_HOME|GROK_APP_ACP|grok_agent_stdio|com\.grokapp\.desktop' -- src src-tauri .github scripts package.json ':!scripts/brand-policy.mjs' ':!scripts/check-brand-policy.test.mjs'
node -e 'const p=require("./testdata/brand-policy/allowed/provider-xai.json"); if(p.provider.id!=="xai") process.exit(1)'
node -e 'const m=require("./testdata/brand-policy/allowed/model-grok.json"); if(m.models[0].id!=="grok-4.5") process.exit(1)'
```

Expected: all commands exit `0`; prohibited production coupling is absent and legal runtime identity fixtures remain intact.

- [ ] **Step 6: Verify history, remotes, and tree cleanliness**

Run:

```bash
git merge-base --is-ancestor d2a2563f19bba46cb67496d3b4ac821a31bceaed HEAD
git merge-base --is-ancestor 05e2aaa96bbdeb68d9740d102ccb1723e26673ed HEAD
git submodule status --recursive
git status --short --branch
```

Expected: both ancestors are present, submodule is clean and pinned, and the only uncommitted file is the verification record being written in Step 7.

- [ ] **Step 7: Write the verification record with actual output summaries**

Create `docs/superpowers/verification/2026-07-28-plan-1-baseline.md` containing:

```md
# Plan 1 Repository and Brand Baseline Verification

- Grok App baseline: d2a2563f19bba46cb67496d3b4ac821a31bceaed
- Frozen design ancestor: 05e2aaa96bbdeb68d9740d102ccb1723e26673ed
- OMP source baseline: 667111575ebba136dadfd6989379e7f67e0d40d9
- Runtime behavior: fail closed (`runtime_unavailable`)
- Brand policy: zero violations
- Provenance policy: passed
- Legal/SBOM input policy: passed
- Frontend typecheck/tests/build: passed
- Rust tests/metadata: passed
- Sidecar bundled: no; scheduled for later plans
```

Append exact test counts and platform used from the fresh command output; do not claim another operating system was tested locally.

- [ ] **Step 8: Commit the verification evidence**

Run:

```bash
git add docs/superpowers/verification/2026-07-28-plan-1-baseline.md
git commit -m "test: verify repository and brand baseline"
git status --short --branch
```

Expected: commit succeeds and the working tree is clean.

---

## Plan 1 Completion Boundary

Plan 1 is complete only when all Task 14 gates pass. The resulting application is intentionally not an operational Agent client: it is a correctly branded, reproducible, buildable, fail-closed shell with the OMP source pinned and all Grok App-specific runtime behavior removed. Plan 2 may then define and implement the versioned OMP Desktop Extension Protocol without inheriting hidden Grok assumptions.

