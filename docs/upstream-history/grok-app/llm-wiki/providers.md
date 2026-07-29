# Custom providers & agent profile

Product rules for **OpenAI-compatible relays** (CPA / sub2api / OneAPI / self-hosted) and how they reach Grok Build.

## Agent transport (shared with Grok Desktop)

Both Grok App and community **Grok Desktop** drive intelligence the same way:

| Layer | Implementation |
|-------|----------------|
| Runtime | **Grok Build CLI** binary (`grok`) |
| Entry | `grok agent stdio` |
| Protocol | **ACP** (Agent Client Protocol) JSON-RPC over stdio |
| Client | Desktop Host (`AcpClient`) — **not** a reimplemented agent brain |

Desktop never reimplements tools/sampling. It is an ACP client + UI shell.

## Agent profile (`GROK_HOME`)

| Session data mode | `GROK_HOME` for spawned agent |
|-------------------|-------------------------------|
| `independent` (default) | `~/.grok-app/agent-home` (or `$GROK_APP_HOME/agent-home`) |
| `shared` | `~/.grok` (CLI default) |

Custom providers are written to **`$GROK_HOME/config.toml`** as `[model.<id>]` sections so the agent can use `base_url` + `api_key` without OAuth fallback.

## Provider model (L2)

| Field | Role |
|-------|------|
| `id` | Config section slug (`[model.<id>]`) |
| `name` | Display label |
| `baseUrl` | OpenAI-compatible root, usually ends with `/v1` |
| `apiKey` | Required for custom relay; never returned plaintext to UI |
| `model` | Request body model id |
| `apiBackend` | Message format: `responses` (default) \| `chat_completions` \| `messages` |
| `isDefault` | Maps to `[models].default` |

CPA / sub2api / grok-go are **not special-cased** — any compatible base URL works.
No bundled third-party presets (e.g. yunyi) ship with the app; users add relays themselves.

## Settings UI (Account → Custom providers)

Left / right split (`ProvidersPanel`):

| Side | Content |
|------|---------|
| Left | **Add provider** on top; list of cards. Official Grok card first **only if** signed in / CLI auth / official key; otherwise list starts empty. |
| Right | Create/edit form when adding or selecting a custom card; official detail when selecting the official card; empty placeholder otherwise. |

Each card has **Use** to activate that route (`providers_activate`). Clicking a card opens detail/edit. No long intro copy, agent-home path, or separate “active route” switcher.

## Route switching (auth isolation)

Grok Build 0.2.x will send **OIDC** when `auth.json` is present — even if the request URL is a custom relay. That produces:

`Unauthorized (401) from https://api.example.com/v1/responses` with `Auth: Oidc`.

Verified working combinations:

| Route | `[models].default` | agent `--model` | agent-home `auth.json` |
|-------|--------------------|-----------------|------------------------|
| Custom relay | provider id (`yunyi`) | **provider id** | **removed** (api_key only) |
| Official | `grok` | catalog id (`grok-4.5`) | **synced** from `~/.grok` |

Host must rebind both sides on every switch and before each ACP spawn (`prepare_route_auth_for_agent` + `agent_spawn_model_id`). Composer model stays a catalog id for the UI; spawn resolves the channel id separately.

## Host commands

| Command | Role |
|---------|------|
| `providers_list` | Providers + default (no raw keys) |
| `providers_upsert` | Create/update; empty key keeps previous |
| `providers_remove` | Delete section |
| `providers_set_default` | Set default model id |
| `providers_ping` | `GET {base}/models` RTT |
| `providers_list_models` | Fetch remote model ids |
| `providers_cc_switch_scan` | Read-only scan of local **CC Switch** Grok Build providers |
| `providers_cc_switch_import` | Import selected CC Switch rows into custom providers |
| `editors_list` | Detected local IDEs |
| `open_in_editor` | Open path in chosen editor |

## Import from CC Switch (#167)

Settings → Account → Custom providers → **Import from CC Switch**.

| Step | Behavior |
|------|----------|
| Detect | Resolve `cc-switch.db` (see paths below); open SQLite **read-only** |
| Scope | `providers` where `app_type = 'grokbuild'` |
| Preview | Multi-select list (no full API keys; status badges) |
| Import | Map TOML → `providers_upsert` into current agent-home `config.toml`; **same id overwrites** (no UI toggle); does not auto-activate route |

### CC Switch data paths (cross-platform)

| Priority | Location |
|----------|----------|
| 1 | `GROK_APP_CC_SWITCH_DIR` or `CC_SWITCH_HOME` env (if set and contains db) |
| 2 | Tauri Store override: `app_config_dir_override` in `app_paths.json` under `com.ccswitch.desktop` |
| 3 | **Default:** `{user_home}/.cc-switch/cc-switch.db` (macOS / Windows / Linux) |
| 4 | Windows only: `{HOME}/.cc-switch/cc-switch.db` when Profile default is missing (v3.10.3 legacy) |

Store file locations:

| OS | `app_paths.json` |
|----|------------------|
| macOS | `~/Library/Application Support/com.ccswitch.desktop/app_paths.json` |
| Windows | `%APPDATA%\com.ccswitch.desktop\app_paths.json` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/com.ccswitch.desktop/app_paths.json` |

Official CC Switch rows are not imported as custom relays. Proxy-takeover placeholders (`PROXY_MANAGED`, `127.0.0.1:…`) are rejected.

## Security

- UI only sees `hasApiKey`.
- Logs must redact keys (existing redact paths).
- Official OAuth (`auth.json`) stays separate from relay keys.

## Sponsorship (L3, future)

Recommended catalog / paid naming sits **above** L2 as templates only. Keys always user-owned. See `docs/分析-Grok-Desktop对照报告.md` §7.
