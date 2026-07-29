import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const GROK_IMPORT_COMMIT = "d2a2563f19bba46cb67496d3b4ac821a31bceaed";
const OMP_PINNED_COMMIT = "667111575ebba136dadfd6989379e7f67e0d40d9";

const ACCEPTED_LICENSES = new Set([
  "MIT",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "Apache-2.0",
  "ISC",
  "Unicode-3.0",
  "CC-BY-4.0",
]);

const CC_BY_4_0_ONLY_PATH = "runtime/oh-my-pi/crates/pi-natives/src/fonts/Silver.LICENSE";

function readInventory() {
  return JSON.parse(fs.readFileSync(path.join(ROOT, "third-party/inventory.json"), "utf8"));
}

export function requiredLegalInputs() {
  return readInventory().map((entry) => ({ ...entry, path: entry.licensePath }));
}

function findOmpLegalFiles() {
  const submodulePath = path.join(ROOT, "runtime/oh-my-pi");
  const output = execFileSync("git", ["-C", submodulePath, "ls-files"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  return output
    .split("\n")
    .filter(Boolean)
    .filter((file) => {
      const base = path.basename(file);
      return base === "LICENSE" || base === "NOTICE" || base.endsWith(".LICENSE");
    })
    .map((file) => `runtime/oh-my-pi/${file}`)
    .sort();
}

export function validateInventory() {
  const errors = [];
  const inventory = readInventory();

  for (const entry of inventory) {
    const fullPath = path.join(ROOT, entry.licensePath);
    if (!fs.existsSync(fullPath)) {
      errors.push(`inventory path does not exist: ${entry.licensePath}`);
    }
  }

  for (const entry of inventory) {
    const expectedCommit = entry.licensePath === "LICENSE" ? GROK_IMPORT_COMMIT : OMP_PINNED_COMMIT;
    if (entry.sourceCommit !== expectedCommit) {
      errors.push(
        `source commit mismatch for ${entry.name}: expected ${expectedCommit}, got ${entry.sourceCommit}`,
      );
    }
  }

  for (const entry of inventory) {
    if (!ACCEPTED_LICENSES.has(entry.license)) {
      errors.push(`unaccepted license "${entry.license}" for ${entry.name} (${entry.licensePath})`);
    }
    if (entry.license === "CC-BY-4.0" && entry.licensePath !== CC_BY_4_0_ONLY_PATH) {
      errors.push(`CC-BY-4.0 restricted to ${CC_BY_4_0_ONLY_PATH}, found at ${entry.licensePath}`);
    }
  }

  if (fs.existsSync(path.join(ROOT, "remote-bridge"))) {
    errors.push("remote-bridge directory must be absent");
  }

  const inventoryPaths = new Set(inventory.map((entry) => entry.licensePath));
  for (const file of findOmpLegalFiles()) {
    if (!inventoryPaths.has(file)) {
      errors.push(`tracked legal file not in inventory: ${file}`);
    }
  }

  return errors;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const errors = validateInventory();
  if (errors.length > 0) {
    for (const error of errors) console.error(error);
    process.exitCode = 1;
  } else {
    console.log("Legal baseline checks passed: inventory, policy, and notice coverage verified.");
  }
}
