# Plugin marketplace (App)

Install and manage Grok Build plugins from Settings → Extensions without dropping to the CLI for day-1 discovery.

## Current behavior

| Action | Where | Effect |
|--------|--------|--------|
| List installed | Extensions → Plugins | `grok plugin list --json` + inspect enrich |
| Enable / disable | Same | CLI + `~/.grok/config.toml` |
| Details / uninstall / update | Same | CLI; GlassModal confirms uninstall |
| Browse catalog | Extensions → Marketplace | `plugin list --json --available` (cached) |
| Install from catalog | Marketplace row → confirm | `plugin install --trust` then `plugin enable` + soft-respawn |
| Manual install | Plugins → “Install from path or git…” | Same install path (path / git / `owner/repo`) |
| Marketplace sources | Marketplace → sources details | add / remove / refresh git sources |

Skills / MCP enable toggles remain App-side (`extensions.json` + ACP inject). **Plugins follow CLI/config as source of truth** — do not invent a second store under `~/.grok-app`.

## Catalog UX

1. **Default filter** is **xAI Official** (about a dozen curated plugins). Other sources (e.g. Claude official) are available under “All sources” or per-source chips.
2. **Cache**: first load runs CLI; re-entering Marketplace within ~6h uses in-memory cache. “Refresh catalog” forces a reload. Install/add/remove sources invalidate or patch the cache.
3. **Install** uses GlassModal confirm (no `window.confirm`). On success the plugin is **trusted and enabled**, then the agent soft-respawns so skills/MCP appear on the next turn.
4. **Empty Plugins tab** links to Marketplace (“Browse official plugins”).

## Component counts

CLI often returns top-level `skill_count: 0` / `has_mcp: false` while `components` is filled. Host parsing (Rust + TS) enriches counts from `components.skills` / `mcpServers` / `hooks` / `agents`.

## Safety

- Never auto-install.
- Install always passes `--trust` for non-interactive UI; confirmation copy states third-party code runs with agent permissions.
- Prefer marketplace name pins (`name@xAI Official`) when the same id exists in multiple catalogs.

## i18n

All user-facing strings under `ext.plugins.*` / `ext.market.*` (en + zh + zh-TW).

## Non-goals

- Publishing plugins from the App.
- Parallel package managers (npm/pip) outside `grok plugin`.
- Hand-maintained catalog under app data (always CLI marketplace sources).
