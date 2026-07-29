# Plan 4: Config, Provider, MCP, Skills, and Secure Credentials Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the v1 protocol handlers for MCP, diagnostics, credentials, skills, and config discovery to real OMP resources, replacing the `runtime_unavailable` stubs left by Plan 2/3.

**Architecture:** Plan 2 defined the `_omp/desktop/v1/*` schema and handlers but left several handlers stubbed (`mcp.list`, `mcp.discover`, `diagnostics.selfCheck`, all `credentials.*`) and two methods missing entirely (`skills.list`, `config.discover`). Plan 3 wired the transport but `OmpExtension::negotiate_capability` is never called so every v1 call still fails closed. Plan 4 closes these gaps: (1) adds the two missing methods, (2) replaces stubs with real OMP-resource backing, (3) bridges the real `AuthStorage` class to the v1 `AuthStorageLike` interface via an adapter, (4) wires capability negotiation in the Rust host, and (5) fixes the `skills_list` Tauri command that currently mis-routes through `extensions.list`.

**Tech Stack:** TypeScript (OMP runtime fork), Rust (Tauri host), JSON Schema 2020-12, Vitest, `cargo test`.

**Working directory:** `/Users/po1nt9/Github/grok-app-main`

**Key existing files (read these before starting):**
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts` — method schema registry
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/ids.ts` — stable ID format rules
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts` — handler factory registry
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/extensions.ts` — pattern to copy for `skills.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/mcp.ts` — MCP handler (already exists, needs real backing)
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/diagnostics.ts` — diagnostics handler (already exists, needs real backing)
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/types.ts` — `HandlerDeps` structural interfaces
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts:315-540` — `buildDesktopV1HandlerDeps` wiring
- `runtime/oh-my-pi/packages/coding-agent/src/mcp/config.ts` — `loadAllMCPConfigs(cwd, options)` real MCP loader
- `runtime/oh-my-pi/packages/coding-agent/src/capability/skill.ts` — `skillCapability` and `Skill` interface
- `runtime/oh-my-pi/packages/coding-agent/src/config.ts` — config discovery (`getConfigDirs`, `findConfigFile`)
- `runtime/oh-my-pi/packages/ai/src/auth-storage.ts` — real `AuthStorage` class (large; key methods: `getAll`, `hasAuth`, `setRuntimeApiKey`, `setConfigApiKey`)
- `src/lib/ompDesktopV1/methods.ts` — frontend `MethodMap`
- `src/lib/ompDesktopV1/index.ts` — frontend `OmpDesktopV1Client`
- `src-tauri/src/omp_desktop_v1/mod.rs` — Rust `OmpExtension` client
- `src-tauri/src/omp_desktop_v1/generated.rs` — Rust mirrored types
- `src-tauri/src/commands.rs` — Tauri commands (search for `skills_list`, `route_through_extension`)
- `src-tauri/src/lib.rs` — Tauri app setup (where `OmpExtension` is constructed)

---

## File Structure

**New files (OMP runtime fork):**
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/skills.ts` — `skills.list` handler
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/config.ts` — `config.discover` handler
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/skills.test.ts` — handler unit tests
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/config.test.ts` — handler unit tests
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/credential-adapter.ts` — bridges real `AuthStorage` → `AuthStorageLike`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/credential-adapter.test.ts` — adapter unit tests

**Modified files (OMP runtime fork):**
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts` — add `skills.list` and `config.discover` schemas
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts` — register new handlers, extend `HandlerDeps`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/types.ts` — add `SkillsLike` and `ConfigLike` interfaces
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` — wire real MCP/diagnostics/credentials/skills/config in `buildDesktopV1HandlerDeps`

**Modified files (Desktop host — Rust):**
- `src-tauri/src/omp_desktop_v1/generated.rs` — add `SkillInfo`, `ConfigSourceInfo` types
- `src-tauri/src/omp_desktop_v1/mod.rs` — implement `negotiate_capability` call path
- `src-tauri/src/commands.rs` — fix `skills_list` to route through `skills.list`
- `src-tauri/src/lib.rs` — call `negotiate_capability` on ACP initialize
- `src-tauri/src/session_manager.rs` — invoke `negotiate_capability` after ACP connect

**Modified files (Desktop host — frontend):**
- `src/lib/ompDesktopV1/methods.ts` — add `skills.list` and `config.discover` to `MethodMap`

---

## Task 1: Add `skills.list` v1 method — schema, handler, types

**Files:**
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts`
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/ids.ts`
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/types.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/skills.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/skills.test.ts`
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts`

- [ ] **Step 1: Add `skill` ID format to `schema/ids.ts`**

Add a new entry to the `idFormats` record in `schema/ids.ts`, after the `mcpSource` entry:

```typescript
	skill: {
		prefix: "skill_",
		pattern: "^skill_[a-z0-9][a-z0-9-]{0,63}$",
		example: "skill_code-review",
		description: "Stable skill identifier (kebab-case body, max 64 chars).",
	},
```

- [ ] **Step 2: Add `SkillInfo` shared type to `schema/methods.ts`**

Add to the `sharedTypes` object in `schema/methods.ts`, after `DiagnosticCheck`:

```typescript
	SkillInfo: {
		type: "object",
		properties: {
			id: { type: "string", pattern: idFormats.skill.pattern },
			name: { type: "string" },
			description: { type: ["string", "null"] },
			level: { type: "string", enum: ["user", "project"] },
			hidden: { type: "boolean" },
		},
		required: ["id", "name", "level", "hidden"],
		additionalProperties: false,
	},
```

- [ ] **Step 3: Add `skills.list` method schema to `schema/methods.ts`**

Add to the `methodSchemas` record, after the `diagnostics.selfCheck` entry (the last entry before the closing `};`):

```typescript
	"skills.list": {
		method: "skills.list",
		methodNamespace: "_omp/desktop/v1",
		params: {
			type: "object",
			properties: {
				cwd: { type: "string" },
			},
			additionalProperties: false,
		},
		result: {
			type: "object",
			properties: {
				skills: { type: "array", items: sharedTypes.SkillInfo },
			},
			required: ["skills"],
			additionalProperties: false,
		},
		errors: ["runtime_unavailable"],
		capability: "skills",
	},
```

- [ ] **Step 4: Add `SkillsLike` interface to `types.ts`**

Add to `types.ts`, after `DiagnosticsLike`:

```typescript
/** Structural interface for skill listing. */
export interface SkillsLike {
	list(cwd?: string): Promise<unknown[]>;
}
```

- [ ] **Step 5: Write the failing test for `skills.list` handler**

Create `handlers/skills.test.ts`:

```typescript
import { describe, it, expect } from "vitest";
import { createSkillsHandlers } from "./skills.ts";

describe("createSkillsHandlers", () => {
	it("skills.list normalizes skill records to SkillInfo shape", async () => {
		const listSkills = async () => [
			{
				name: "code-review",
				description: "Reviews code for bugs",
				level: "user",
				frontmatter: { hide: true },
				_source: { level: "user" },
			},
			{
				name: "test-gen",
				description: null,
				level: "project",
				frontmatter: {},
				_source: { level: "project" },
			},
		];
		const handlers = createSkillsHandlers(listSkills);
		const result = await handlers["skills.list"]({ cwd: undefined });
		expect(result.skills).toHaveLength(2);
		expect(result.skills[0]).toEqual({
			id: "skill_code-review",
			name: "code-review",
			description: "Reviews code for bugs",
			level: "user",
			hidden: true,
		});
		expect(result.skills[1]).toEqual({
			id: "skill_test-gen",
			name: "test-gen",
			description: null,
			level: "project",
			hidden: false,
		});
	});

	it("skills.list passes cwd through to listSkills", async () => {
		let receivedCwd: string | undefined;
		const listSkills = async (cwd?: string) => {
			receivedCwd = cwd;
			return [];
		};
		const handlers = createSkillsHandlers(listSkills);
		await handlers["skills.list"]({ cwd: "/project" });
		expect(receivedCwd).toBe("/project");
	});
});
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/handlers/skills.test.ts 2>&1 | tail -20`
Expected: FAIL with "Cannot find module './skills.ts'" or similar.

- [ ] **Step 7: Implement `handlers/skills.ts`**

Create `handlers/skills.ts`:

```typescript
/**
 * Handler for `_omp/desktop/v1/skills.list`.
 *
 * Wraps the OMP skill capability loader in the v1 envelope. Normalises
 * each `Skill` record to the v1 `SkillInfo` shape, deriving a stable
 * `skill_<name>` ID and surfacing the `hidden` flag from frontmatter.
 */

function normalizeSkill(s: any) {
	const name = s.name;
	return {
		id: `skill_${name}`,
		name,
		description: typeof s.description === "string" ? s.description : null,
		level: s.level ?? s._source?.level ?? "user",
		hidden: !!(s.frontmatter?.hide ?? s.frontmatter?.disableModelInvocation),
	};
}

export function createSkillsHandlers(listSkills: (cwd?: string) => Promise<any[]>) {
	return {
		"skills.list": async (params: { cwd?: string }) => {
			const skills = await listSkills(params.cwd);
			return { skills: skills.map(normalizeSkill) };
		},
	};
}
```

- [ ] **Step 8: Run test to verify it passes**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/handlers/skills.test.ts 2>&1 | tail -20`
Expected: PASS (2 tests).

- [ ] **Step 9: Register skills handler in `handlers/index.ts`**

In `handlers/index.ts`:
1. Add import at top with the other handler imports:
   ```typescript
   import { createSkillsHandlers } from "./skills.ts";
   ```
2. Add `skills: SkillsLike;` to the `HandlerDeps` interface (after `diagnostics: DiagnosticsLike;`).
3. Add the import for `SkillsLike` in the type import from `../types.ts`.
4. Add the registration block inside `createAllHandlers`, after the diagnostics block and before `return handlers;`:
   ```typescript
	for (const [name, handler] of Object.entries(
		createSkillsHandlers(deps.skills.list),
	)) {
		handlers.set(name, handler as Handler);
	}
   ```
5. Add `createSkillsHandlers` to the re-export block at the bottom.

- [ ] **Step 10: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/
git commit -m "feat: add skills.list v1 method with schema, handler, and tests"
```

---

## Task 2: Add `config.discover` v1 method — schema, handler, types

**Files:**
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts`
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/types.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/config.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/config.test.ts`
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts`

- [ ] **Step 1: Add `ConfigSourceInfo` shared type to `schema/methods.ts`**

Add to the `sharedTypes` object, after `SkillInfo`:

```typescript
	ConfigSourceInfo: {
		type: "object",
		properties: {
			kind: {
				type: "string",
				enum: ["settings", "mcp", "models", "credentials", "skills", "sessions", "project"],
			},
			path: { type: "string" },
			level: { type: "string", enum: ["user", "project"] },
			writable: { type: "boolean" },
		},
		required: ["kind", "path", "level", "writable"],
		additionalProperties: false,
	},
	ConfigDiscoveryResult: {
		type: "object",
		properties: {
			agentDir: { type: "string" },
			profile: { type: ["string", "null"] },
			projectCwd: { type: ["string", "null"] },
			sources: { type: "array", items: sharedTypes.ConfigSourceInfo },
		},
		required: ["agentDir", "profile", "projectCwd", "sources"],
		additionalProperties: false,
	},
```

- [ ] **Step 2: Add `config.discover` method schema to `schema/methods.ts`**

Add to the `methodSchemas` record, after the `skills.list` entry:

```typescript
	"config.discover": {
		method: "config.discover",
		methodNamespace: "_omp/desktop/v1",
		params: {
			type: "object",
			properties: {
				cwd: { type: "string" },
			},
			additionalProperties: false,
		},
		result: {
			...sharedTypes.ConfigDiscoveryResult,
		},
		errors: ["runtime_unavailable"],
		capability: "config",
	},
```

Note: the result schema IS the `ConfigDiscoveryResult` shape directly (not wrapped in `{ config: ... }`), so the handler returns the discovery result object itself.

- [ ] **Step 3: Add `ConfigLike` interface to `types.ts`**

Add to `types.ts`, after `SkillsLike`:

```typescript
/** Structural interface for config discovery. */
export interface ConfigLike {
	discover(cwd?: string): Promise<{
		agentDir: string;
		profile: string | null;
		projectCwd: string | null;
		sources: Array<{
			kind: string;
			path: string;
			level: string;
			writable: boolean;
		}>;
	}>;
}
```

- [ ] **Step 4: Write the failing test for `config.discover` handler**

Create `handlers/config.test.ts`:

```typescript
import { describe, it, expect } from "vitest";
import { createConfigHandlers } from "./config.ts";

describe("createConfigHandlers", () => {
	it("config.discover returns the discovery result directly", async () => {
		const discover = async () => ({
			agentDir: "/home/.omp/agent",
			profile: "default",
			projectCwd: "/project",
			sources: [
				{ kind: "settings", path: "/home/.omp/agent/settings.yml", level: "user", writable: true },
				{ kind: "mcp", path: "/project/.omp/mcp.json", level: "project", writable: true },
			],
		});
		const handlers = createConfigHandlers(discover);
		const result = await handlers["config.discover"]({ cwd: "/project" });
		expect(result).toEqual({
			agentDir: "/home/.omp/agent",
			profile: "default",
			projectCwd: "/project",
			sources: [
				{ kind: "settings", path: "/home/.omp/agent/settings.yml", level: "user", writable: true },
				{ kind: "mcp", path: "/project/.omp/mcp.json", level: "project", writable: true },
			],
		});
	});

	it("config.discover passes cwd through", async () => {
		let receivedCwd: string | undefined;
		const discover = async (cwd?: string) => {
			receivedCwd = cwd;
			return { agentDir: "/x", profile: null, projectCwd: cwd ?? null, sources: [] };
		};
		const handlers = createConfigHandlers(discover);
		await handlers["config.discover"]({ cwd: "/foo" });
		expect(receivedCwd).toBe("/foo");
	});
});
```

- [ ] **Step 5: Run test to verify it fails**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/handlers/config.test.ts 2>&1 | tail -20`
Expected: FAIL with "Cannot find module './config.ts'".

- [ ] **Step 6: Implement `handlers/config.ts`**

Create `handlers/config.ts`:

```typescript
/**
 * Handler for `_omp/desktop/v1/config.discover`.
 *
 * Delegates to a `discover` function that resolves the Active OMP Agent
 * Directory, the active profile, the project cwd, and the list of
 * configuration sources (settings, MCP, models, credentials, skills,
 * sessions, project). The Desktop host calls this to display the
 * diagnostics page and does NOT replicate the discovery algorithm.
 */

export function createConfigHandlers(discover: (cwd?: string) => Promise<any>) {
	return {
		"config.discover": async (params: { cwd?: string }) => {
			return await discover(params.cwd);
		},
	};
}
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/handlers/config.test.ts 2>&1 | tail -20`
Expected: PASS (2 tests).

- [ ] **Step 8: Register config handler in `handlers/index.ts`**

In `handlers/index.ts`:
1. Add import: `import { createConfigHandlers } from "./config.ts";`
2. Add `config: ConfigLike;` to `HandlerDeps` (after `skills: SkillsLike;`).
3. Add `ConfigLike` to the type import from `../types.ts`.
4. Add registration block after the skills block:
   ```typescript
	for (const [name, handler] of Object.entries(
		createConfigHandlers(deps.config.discover),
	)) {
		handlers.set(name, handler as Handler);
	}
   ```
5. Add `createConfigHandlers` to the re-export block.

- [ ] **Step 9: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/
git commit -m "feat: add config.discover v1 method with schema, handler, and tests"
```

---

## Task 3: Wire `mcp.list`/`mcp.discover` to real `loadAllMCPConfigs`

**Files:**
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` (the `buildDesktopV1HandlerDeps` function, around lines 473-477)

- [ ] **Step 1: Read the current MCP stub in `acp-agent.ts`**

Read `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` lines 470-480. The current code is:
```typescript
	// MCP — keep stubbed. The MCP source registry lands in Plan 4.
	const mcp = {
		list: unavailable,
		discover: unavailable,
	};
```

- [ ] **Step 2: Check what imports are available at the top of `acp-agent.ts`**

Search for existing imports of `loadAllMCPConfigs` in `acp-agent.ts`. If not present, check the import path used elsewhere in the `coding-agent` package. The function lives at `../mcp/config.ts` relative to `modes/acp/`. The correct import path from `acp-agent.ts` is `../../mcp/config.ts`.

- [ ] **Step 3: Add the import for `loadAllMCPConfigs`**

If not already imported, add near the top of `acp-agent.ts` with the other `coding-agent` imports:

```typescript
import { loadAllMCPConfigs } from "../../mcp/config.ts";
```

- [ ] **Step 4: Replace the MCP stub with real backing**

Replace the stub block (lines ~473-477) with:

```typescript
	// MCP — backed by the real `loadAllMCPConfigs` capability loader.
	// `list` returns currently-configured sources (user-level, no project
	// override); `discover` includes project-level sources for the given
	// cwd. Both normalise to the v1 `McpSourceInfo` shape via the handler.
	const mcp = {
		list: async (cwd?: string) => {
			const result = await loadAllMCPConfigs(cwd ?? process.cwd(), {
				enableProjectConfig: false,
			});
			return Object.entries(result.configs).map(([name, config]: [string, any]) => ({
				id: `mcp_${crypto.createHash("sha1").update(name).digest("hex")}`,
				name,
				sourceType: config.type ?? "unknown",
			}));
		},
		discover: async (cwd: string) => {
			const result = await loadAllMCPConfigs(cwd, {
				enableProjectConfig: true,
			});
			return Object.entries(result.configs).map(([name, config]: [string, any]) => ({
				id: `mcp_${crypto.createHash("sha1").update(name).digest("hex")}`,
				name,
				sourceType: config.type ?? "unknown",
			}));
		},
	};
```

If `crypto` is not already imported at the top of `acp-agent.ts`, add: `import { createHash } from "node:crypto";` and use `createHash` instead of `crypto.createHash`.

- [ ] **Step 5: Verify the existing dispatcher tests still pass**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/ 2>&1 | tail -30`
Expected: PASS — the handler unit tests use mock deps, not the real wiring, so they are unaffected.

- [ ] **Step 6: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts
git commit -m "feat: wire mcp.list and mcp.discover to real loadAllMCPConfigs"
```

---

## Task 4: Wire `diagnostics.selfCheck` to real diagnostic checks

**Files:**
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` (the `buildDesktopV1HandlerDeps` function, the `diagnostics` dep)

- [ ] **Step 1: Read the current diagnostics stub**

In `acp-agent.ts`, search for `diagnostics` within `buildDesktopV1HandlerDeps`. Currently the diagnostics dep is likely stubbed or returns an empty array. Find the block that sets `diagnostics`.

- [ ] **Step 2: Implement a real `selfCheck` function**

The diagnostic checks should verify: (1) the active agent directory is accessible, (2) the credential store (if present) is healthy, (3) MCP servers are loadable, (4) the model registry has at least one model. Replace the diagnostics dep with:

```typescript
	// Diagnostics — run a real self-check against the active session's
	// resources. Each check returns `{ name, status, detail }`.
	const diagnostics = {
		selfCheck: async () => {
			const checks: Array<{ name: string; status: string; detail: string | null }> = [];
			const session = sessionLookup();

			// Check 1: Agent directory accessible
			try {
				const { getConfigDirs } = await import("../../config.ts");
				const dirs = getConfigDirs("");
				checks.push({
					name: "agent_directory",
					status: dirs.length > 0 ? "ok" : "warning",
					detail: dirs[0] ?? null,
				});
			} catch (e: any) {
				checks.push({ name: "agent_directory", status: "error", detail: e.message });
			}

			// Check 2: Model registry populated
			if (session) {
				const models = session.modelRegistry.getAvailable();
				checks.push({
					name: "model_registry",
					status: models.length > 0 ? "ok" : "warning",
					detail: `${models.length} models available`,
				});
			} else {
				checks.push({
					name: "model_registry",
					status: "warning",
					detail: "no active session",
				});
			}

			// Check 3: MCP configs loadable
			try {
				const { loadAllMCPConfigs } = await import("../../mcp/config.ts");
				const result = await loadAllMCPConfigs(process.cwd());
				checks.push({
					name: "mcp_config",
					status: "ok",
					detail: `${Object.keys(result.configs).length} MCP sources`,
				});
			} catch (e: any) {
				checks.push({ name: "mcp_config", status: "error", detail: e.message });
			}

			// Check 4: Auth storage accessible
			const authStorage = sessionLookup()?.modelRegistry.authStorage;
			checks.push({
				name: "auth_storage",
				status: authStorage ? "ok" : "warning",
				detail: authStorage ? "available" : "not configured",
			});

			return checks;
		},
	};
```

- [ ] **Step 3: Verify the dispatcher tests pass**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/ 2>&1 | tail -30`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts
git commit -m "feat: wire diagnostics.selfCheck to real resource checks"
```

---

## Task 5: Implement credential-mgmt adapter bridging `AuthStorage` → `AuthStorageLike`

**Files:**
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/credential-adapter.ts`
- Create: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/credential-adapter.test.ts`
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` (replace the stub `authStorage` dep)

- [ ] **Step 1: Read the real `AuthStorage` class to identify available methods**

Read `runtime/oh-my-pi/packages/ai/src/auth-storage.ts` around the `AuthStorage` class (line 1231+). Identify methods that can serve the v1 adapter:
- `getAll(provider)` → returns `StoredCredential[]` — use for `listMetadata`
- `hasAuth(provider)` → returns boolean — use for health check
- The class does NOT expose `beginAuth`/`completeAuth`/`cancelAuth`/`replace`/`revoke`/`migrationStatus` directly. These surface `runtime_unavailable` until a future plan adds the full auth-broker. The adapter MUST expose them but throw `DesktopV1Error("runtime_unavailable", ...)` for the unimplemented ones.

- [ ] **Step 2: Write the failing test for the credential adapter**

Create `credential-adapter.test.ts`:

```typescript
import { describe, it, expect } from "vitest";
import { adaptAuthStorage } from "./credential-adapter.ts";
import { DesktopV1Error } from "./errors.ts";

describe("adaptAuthStorage", () => {
	it("listMetadata returns credential metadata without secrets", async () => {
		const fakeAuthStorage = {
			getAll: async () => [
				{ provider: "xai", type: "api_key", apiKey: "sk-secret-123" },
				{ provider: "xai", type: "oauth", accessToken: "tok-abc", refreshToken: "rt-def" },
			],
			hasAuth: async () => true,
		};
		const adapted = adaptAuthStorage(fakeAuthStorage as any);
		const metadata = await adapted.listMetadata("xai") as any[];
		expect(metadata).toHaveLength(2);
		expect(metadata[0]).toEqual({ id: "cred_xai_api_key_0", providerId: "xai", status: "active" });
		expect(metadata[1]).toEqual({ id: "cred_xai_oauth_1", providerId: "xai", status: "active" });
		// No secrets leaked
		expect(JSON.stringify(metadata)).not.toContain("sk-secret-123");
		expect(JSON.stringify(metadata)).not.toContain("tok-abc");
	});

	it("listMetadata without providerId returns all providers", async () => {
		let calledProvider: string | undefined;
		const fakeAuthStorage = {
			getAll: async (provider?: string) => {
				calledProvider = provider;
				return provider ? [{ provider, type: "api_key" }] : [];
			},
			hasAuth: async () => false,
		};
		const adapted = adaptAuthStorage(fakeAuthStorage as any);
		await adapted.listMetadata();
		expect(calledProvider).toBeUndefined();
	});

	it("unimplemented methods throw runtime_unavailable", async () => {
		const fakeAuthStorage = { getAll: async () => [], hasAuth: async () => false };
		const adapted = adaptAuthStorage(fakeAuthStorage as any);
		await expect(adapted.beginAuth("xai", "api_key")).rejects.toThrow(DesktopV1Error);
		await expect(adapted.completeAuth("id", "code")).rejects.toThrow(DesktopV1Error);
		await expect(adapted.cancelAuth("id")).rejects.toThrow(DesktopV1Error);
		await expect(adapted.replace("id")).rejects.toThrow(DesktopV1Error);
		await expect(adapted.revoke("id")).rejects.toThrow(DesktopV1Error);
	});

	it("health returns healthy/unhealthy lists based on hasAuth", async () => {
		const fakeAuthStorage = {
			getAll: async () => [
				{ provider: "xai", type: "api_key" },
				{ provider: "google", type: "oauth" },
			],
			hasAuth: async (provider: string) => provider === "xai",
		};
		const adapted = adaptAuthStorage(fakeAuthStorage as any);
		const result = await adapted.health();
		expect(result.healthy).toContain("cred_xai_api_key_0");
		expect(result.unhealthy).toContain("cred_google_oauth_0");
	});

	it("migrationStatus returns zeros (no migration yet)", async () => {
		const fakeAuthStorage = { getAll: async () => [], hasAuth: async () => false };
		const adapted = adaptAuthStorage(fakeAuthStorage as any);
		const status = await adapted.migrationStatus();
		expect(status).toEqual({ migrated: 0, pending: 0, failed: 0, details: [] });
	});
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/credential-adapter.test.ts 2>&1 | tail -20`
Expected: FAIL with "Cannot find module './credential-adapter.ts'".

- [ ] **Step 4: Implement `credential-adapter.ts`**

Create `credential-adapter.ts`:

```typescript
/**
 * Credential management adapter.
 *
 * Bridges the real OMP `AuthStorage` class (which exposes `getAll`,
 * `hasAuth`, etc.) to the v1 `AuthStorageLike` interface (which expects
 * `listMetadata`, `beginAuth`, `completeAuth`, etc.).
 *
 * The real `AuthStorage` does not yet expose full auth-broker methods
 * (`beginAuth`/`completeAuth`/`cancelAuth`/`replace`/`revoke`). Until a
 * future plan adds the auth-broker, those methods throw
 * `DesktopV1Error("runtime_unavailable")` so the host fails closed
 * instead of silently returning empty results.
 *
 * **Security invariant:** `listMetadata` NEVER returns the secret. The
 * `credentials.list` handler strips secret-bearing fields as a second
 * line of defense, but this adapter never puts them in the metadata
 * object in the first place.
 */

import { DesktopV1Error } from "./errors.ts";

/** Generate a stable credential ID from provider + type + index. */
function makeCredentialId(provider: string, type: string, index: number): string {
	// Use a simple hash to get a 26-char base32 body. Not cryptographic —
	// just needs to be stable and match the cred_[a-z2-7]{26} pattern.
	const input = `${provider}:${type}:${index}`;
	let hash = 0;
	for (let i = 0; i < input.length; i++) {
		hash = ((hash << 5) - hash + input.charCodeAt(i)) | 0;
	}
	hash = Math.abs(hash);
	const alphabet = "abcdefghijklmnopqrstuvwxyz234567";
	let body = "";
	let n = hash;
	for (let i = 0; i < 26; i++) {
		body += alphabet[n % 32];
		n = Math.floor(n / 32);
		if (n === 0) n = hash + i; // pad to 26 chars
	}
	return `cred_${body}`;
}

export function adaptAuthStorage(realAuthStorage: any): import("./types.ts").AuthStorageLike {
	const unavailable = async (): Promise<never> => {
		throw new DesktopV1Error("runtime_unavailable", {
			reason: "auth-broker not yet implemented for this credential operation",
		});
	};

	return {
		listMetadata: async (providerId?: string) => {
			const all = await realAuthStorage.getAll(providerId);
			return all.map((cred: any, index: number) => ({
				id: makeCredentialId(cred.provider ?? providerId ?? "unknown", cred.type ?? "unknown", index),
				providerId: cred.provider ?? providerId ?? "unknown",
				status: "active" as const,
			}));
		},
		beginAuth: unavailable,
		completeAuth: unavailable,
		cancelAuth: unavailable,
		replace: unavailable,
		revoke: unavailable,
		health: async (credentialId?: string) => {
			const all = await realAuthStorage.getAll(undefined);
			const healthy: string[] = [];
			const unhealthy: string[] = [];
			for (let i = 0; i < all.length; i++) {
				const cred = all[i];
				const id = makeCredentialId(cred.provider ?? "unknown", cred.type ?? "unknown", i);
				if (credentialId && id !== credentialId) continue;
				const ok = await realAuthStorage.hasAuth(cred.provider);
				if (ok) {
					healthy.push(id);
				} else {
					unhealthy.push(id);
				}
			}
			return { healthy, unhealthy };
		},
		migrationStatus: async () => ({
			migrated: 0,
			pending: 0,
			failed: 0,
			details: [],
		}),
	};
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/credential-adapter.test.ts 2>&1 | tail -20`
Expected: PASS (5 tests).

- [ ] **Step 6: Wire the adapter into `acp-agent.ts`**

In `acp-agent.ts`, replace the stub `authStorage` block (around lines 461-471) with:

```typescript
	// Auth storage — bridge the real `AuthStorage` to the v1
	// `AuthStorageLike` interface via the credential adapter. The real
	// `AuthStorage` exposes `getAll`/`hasAuth` but not the full
	// auth-broker surface; the adapter surfaces `runtime_unavailable`
	// for unimplemented methods until a future plan adds them.
	const realAuthStorage = sessionLookup()?.modelRegistry.authStorage;
	const authStorage = realAuthStorage
		? adaptAuthStorage(realAuthStorage)
		: {
				listMetadata: unavailable,
				beginAuth: unavailable,
				completeAuth: unavailable,
				cancelAuth: unavailable,
				replace: unavailable,
				revoke: unavailable,
				health: unavailable,
				migrationStatus: unavailable,
			};
```

Add the import at the top of `acp-agent.ts`:

```typescript
import { adaptAuthStorage } from "./desktop-v1/credential-adapter.ts";
```

- [ ] **Step 7: Verify all desktop-v1 tests pass**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/ 2>&1 | tail -30`
Expected: PASS (all tests).

- [ ] **Step 8: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/
git commit -m "feat: add credential adapter bridging AuthStorage to v1 AuthStorageLike"
```

---

## Task 6: Wire `skills.list` and `config.discover` backing in `buildDesktopV1HandlerDeps`

**Files:**
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts`

- [ ] **Step 1: Add imports for skill loading and config discovery**

At the top of `acp-agent.ts`, add:

```typescript
import { loadCapability } from "../../discovery.ts";
import { skillCapability } from "../../capability/skill.ts";
import { getConfigDirs } from "../../config.ts";
```

If any of these are already imported, skip the duplicate.

- [ ] **Step 2: Add the `skills` dep to `buildDesktopV1HandlerDeps`**

After the `mcp` dep block (added in Task 3), add:

```typescript
	// Skills — backed by the real `skillCapability` loader. Returns
	// user-level and project-level skills for the given cwd.
	const skills = {
		list: async (cwd?: string) => {
			const result = await loadCapability(skillCapability.id, { cwd: cwd ?? process.cwd() });
			return result.items;
		},
	};
```

- [ ] **Step 3: Add the `config` dep to `buildDesktopV1HandlerDeps`**

After the `skills` dep block, add:

```typescript
	// Config discovery — resolve the Active OMP Agent Directory, the
	// active profile, and the list of configuration sources. Desktop
	// calls this to display the diagnostics page and does NOT replicate
	// the discovery algorithm.
	const config = {
		discover: async (cwd?: string) => {
			const dirs = getConfigDirs("");
			const agentDir = dirs[0] ?? process.cwd();
			const profile = process.env.OMP_PROFILE ?? null;
			const projectCwd = cwd ?? null;
			const sources = dirs.map(d => ({
				kind: "settings" as const,
				path: d,
				level: "user" as const,
				writable: true,
			}));
			if (cwd) {
				sources.push({
					kind: "project",
					path: `${cwd}/.omp`,
					level: "project",
					writable: true,
				});
			}
			return { agentDir, profile, projectCwd, sources };
		},
	};
```

- [ ] **Step 4: Verify the dispatcher tests pass**

Run: `cd runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/ 2>&1 | tail -30`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts
git commit -m "feat: wire skills.list and config.discover to real OMP resources"
```

---

## Task 7: Wire `negotiate_capability` in the Rust host

**Files:**
- Modify: `src-tauri/src/omp_desktop_v1/mod.rs`
- Modify: `src-tauri/src/session_manager.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Read the current `OmpExtension` client**

Read `src-tauri/src/omp_desktop_v1/mod.rs`. The `negotiate_capability` method exists but is never called. The `request` method returns `runtime_unavailable` when capability is `None`.

- [ ] **Step 2: Read the session manager's connect flow**

Read `src-tauri/src/session_manager.rs` to find where the ACP `initialize` response is received. After `initialize`, the runtime sends back a capability descriptor. We need to extract the `_omp/desktop/v1` capability from the `initialize` result and call `negotiate_capability`.

- [ ] **Step 3: Add a method to extract v1 capability from ACP initialize result**

In `src-tauri/src/omp_desktop_v1/mod.rs`, add a helper function:

```rust
/// Extract the `_omp/desktop/v1` capability descriptor from an ACP
/// `initialize` result, if the runtime advertised it.
///
/// The ACP `initialize` response may include an `extensions` array
/// containing capability descriptors. We look for one whose namespace
/// matches `_omp/desktop/v1` and deserialize it into
/// `DesktopV1Capability`.
pub fn extract_capability_from_initialize(
    initialize_result: &serde_json::Value,
) -> Option<DesktopV1Capability> {
    let extensions = initialize_result.get("extensions")?.as_array()?;
    for ext in extensions {
        let namespace = ext.get("namespace")?.as_str()?;
        if namespace == "_omp/desktop/v1" {
            if let Ok(cap) = serde_json::from_value::<DesktopV1Capability>(ext.clone()) {
                return Some(cap);
            }
        }
    }
    None
}
```

- [ ] **Step 4: Wire `negotiate_capability` in the session manager**

In `src-tauri/src/session_manager.rs`, find the `connect_inner` method (or wherever the ACP `initialize` response is handled). After the `initialize` call succeeds, extract the v1 capability and call `negotiate_capability` on the `OmpExtension` shared state.

Add the call after the `initialize` result is received:

```rust
// After ACP initialize succeeds, negotiate v1 capability
if let Some(cap) = omp_desktop_v1::extract_capability_from_initialize(&initialize_result) {
    omp_extension.negotiate_capability(Some(cap)).await;
    log::info!("OMP Desktop v1 capability negotiated: {} methods", 
        omp_extension.capability().await.map(|c| c.methods.len()).unwrap_or(0));
} else {
    log::warn!("OMP Runtime did not advertise _omp/desktop/v1 capability");
    omp_extension.negotiate_capability(None).await;
}
```

The exact insertion point depends on the existing `connect_inner` structure — read the file to find where `initialize_result` is available. The `omp_extension` must be accessible as a shared `Arc<OmpExtension>` from the app state.

- [ ] **Step 5: Ensure `OmpExtension` is registered as Tauri state**

In `src-tauri/src/lib.rs`, verify that `OmpExtension` is wrapped in `Arc` and managed as Tauri state. If `negotiate_capability` is called from `session_manager`, the `OmpExtension` must be cloned from the `Arc` and passed to the session manager, or the session manager must receive a reference to it.

Read `src-tauri/src/lib.rs` to see how state is currently managed and adjust so `session_manager` can access `OmpExtension`.

- [ ] **Step 6: Run Rust tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test 2>&1 | tail -30`
Expected: PASS (existing tests still pass; new code is wired but not yet exercised by tests — that's OK, the fail-closed path is the default).

- [ ] **Step 7: Verify the build compiles**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo build 2>&1 | tail -20`
Expected: PASS (no compile errors).

- [ ] **Step 8: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/
git commit -m "feat: wire negotiate_capability in Rust host after ACP initialize"
```

---

## Task 8: Fix `skills_list` Tauri command and update frontend types

**Files:**
- Modify: `src-tauri/src/commands.rs` (the `skills_list` command)
- Modify: `src/lib/ompDesktopV1/methods.ts` (add `skills.list` and `config.discover` to `MethodMap`)

- [ ] **Step 1: Read the current `skills_list` command**

Read `src-tauri/src/commands.rs` around line 1834. The command currently calls `route_through_extension(&state, "extensions.list", params)` which returns extensions, not skills.

- [ ] **Step 2: Fix the routing**

Change the method name from `"extensions.list"` to `"skills.list"` in the `skills_list` command:

```rust
#[tauri::command]
pub async fn skills_list(
    state: State<'_, Arc<OmpExtension>>,
    cwd: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "cwd": cwd });
    route_through_extension(&state, "skills.list", params).await
}
```

The exact signature may differ — read the current implementation and only change the method string and params.

- [ ] **Step 3: Add `SkillInfo` and `ConfigSourceInfo` types to Rust `generated.rs`**

In `src-tauri/src/omp_desktop_v1/generated.rs`, add after `ExtensionInfo`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub level: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSourceInfo {
    pub kind: String,
    pub path: String,
    pub level: String,
    pub writable: bool,
}
```

- [ ] **Step 4: Add `skills.list` and `config.discover` to the frontend `MethodMap`**

In `src/lib/ompDesktopV1/methods.ts`, add the param/result interfaces before the `MethodMap`:

```typescript
// ── skills.list ────────────────────────────────────────────────────────────
export interface SkillsListParams {
  cwd?: string;
}
export interface SkillInfo {
  id: string;
  name: string;
  description: string | null;
  level: "user" | "project";
  hidden: boolean;
}
export interface SkillsListResult {
  skills: SkillInfo[];
}

// ── config.discover ─────────────────────────────────────────────────────────
export interface ConfigDiscoverParams {
  cwd?: string;
}
export interface ConfigSourceInfo {
  kind: "settings" | "mcp" | "models" | "credentials" | "skills" | "sessions" | "project";
  path: string;
  level: "user" | "project";
  writable: boolean;
}
export interface ConfigDiscoverResult {
  agentDir: string;
  profile: string | null;
  projectCwd: string | null;
  sources: ConfigSourceInfo[];
}
```

Add to the `MethodMap` interface:

```typescript
  "skills.list": { params: SkillsListParams; result: SkillsListResult };
  "config.discover": { params: ConfigDiscoverParams; result: ConfigDiscoverResult };
```

- [ ] **Step 5: Run frontend type check**

Run: `cd /Users/po1nt9/Github/grok-app-main && npx tsc --noEmit 2>&1 | tail -20`
Expected: PASS (no type errors).

- [ ] **Step 6: Run Rust build**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo build 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/commands.rs src-tauri/src/omp_desktop_v1/generated.rs src/lib/ompDesktopV1/methods.ts
git commit -m "fix: route skills_list through skills.list v1 method and add frontend types"
```

---

## Task 9: Final verification and record

**Files:**
- Create: `docs/superpowers/verification/2026-07-29-plan-4-config-provider-mcp-skills-credentials.md`

- [ ] **Step 1: Run the brand policy scanner**

Run: `cd /Users/po1nt9/Github/grok-app-main && node scripts/check-brand-policy.mjs 2>&1 | tail -20`
Expected: PASS (zero violations in production code).

- [ ] **Step 2: Run the OMP runtime desktop-v1 tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/runtime/oh-my-pi && npx vitest run packages/coding-agent/src/modes/acp/desktop-v1/ 2>&1 | tail -30`
Expected: PASS (all tests, including new skills/config/credential-adapter tests).

- [ ] **Step 3: Run the frontend type check and tests**

Run: `cd /Users/po1nt9/Github/grok-app-main && npx tsc --noEmit 2>&1 | tail -10 && npx vitest run 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 4: Run the Rust tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test 2>&1 | tail -20`
Expected: PASS (existing tests; new wiring doesn't break anything).

- [ ] **Step 5: Update provenance**

Update `provenance/omp-patches.json` to add the Plan 4 patch entry:

```json
{
  "id": "plan-4-config-provider-mcp-skills-credentials",
  "branch": "desktop-v1-protocol",
  "description": "Plan 4: wire v1 handlers to real OMP resources (MCP, diagnostics, credentials, skills, config)",
  "plan": "2026-07-29-plan-4-config-provider-mcp-skills-credentials",
  "commit": "<HEAD_SHA>"
}
```

- [ ] **Step 6: Write the verification record**

Create `docs/superpowers/verification/2026-07-29-plan-4-config-provider-mcp-skills-credentials.md` with:
- Date and plan reference
- Tasks completed (1-9)
- Test results (brand policy, vitest, tsc, cargo test)
- Files changed summary
- Known gaps (auth-broker methods surface `runtime_unavailable` by design until a future plan adds full OAuth flows)

- [ ] **Step 7: Commit and push**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add docs/superpowers/verification/ provenance/
git commit -m "test: verify Plan 4 config, provider, MCP, skills, credentials wiring"
git push origin feat/rename-desktop-release-surfaces
```

---

## Self-Review

**1. Spec coverage:**
- Master design §7 (Active Directory discovery): covered by Task 2 (`config.discover`).
- Master design §8 (Credentials): covered by Task 5 (credential adapter). Full auth-broker deferred — surfaces `runtime_unavailable` by design.
- Master design §9 (MCP, Skills, slash commands): MCP covered by Task 3, Skills covered by Task 1+6. Slash commands are out of scope for Plan 4 (they're a UI concern in Plan 5).
- Master design §5.4 (Provider/Model/Credential API): providers/models already wired in Plan 2. Credentials wired in Task 5. Session config already wired in Plan 2.
- Plans 4-10 roadmap task list items 1-6: all covered by Tasks 1-8.

**2. Placeholder scan:** No TBD/TODO/"implement later" found. All code blocks contain real implementation.

**3. Type consistency:**
- `SkillInfo` shape is consistent across schema, handler, and frontend types.
- `ConfigSourceInfo` / `ConfigDiscoverResult` consistent across schema, handler, and frontend.
- `AuthStorageLike` interface matches what `adaptAuthStorage` returns.
- `SkillsLike` and `ConfigLike` added to `HandlerDeps` and wired in `createAllHandlers`.
