# Remote access risk

Security posture of OMP Desktop's Remote IM bridge — which channels exist,
how inbound messages are verified, and the knobs you must set before exposing
a bridge.

> **Current status (2026-07-31):** 14 channel adapters are implemented under
> `src-tauri/src/remote_im/channels/`. The official 1.0 support tier is **10
> fixed channels + Weixin (personal) conditional** (design §14.1); the rest are
> best-effort. Remote approvals (yolo) are TTL-bound and never persisted.

## 1. Transport per channel

| Transport | Channels | Inbound authentication |
|---|---|---|
| Platform-authenticated WebSocket / long-poll | dingtalk, discord, feishu, matrix, qq, qqbot, slack (Socket Mode), telegram, weibo, weixin (ilink), wps_xiezuo, wecom (WS mode) | the platform session itself — no per-message signature |
| Local webhook server (binds a port) | **LINE**, **wecom** (`connect_mode=webhook`) | per-request signature (§2) |

Webhook servers bind `127.0.0.1` by default. `allow_external` is opt-in and
assumes you put a TLS-terminating reverse proxy/tunnel in front.

## 2. Signature verification

- **LINE** — `X-Line-Signature` HMAC-SHA256 over the raw body, verified
  against the channel secret.
- **WeCom webhook** — `msg_signature` SHA1 verification when `callback_token`
  is configured.

**Honest boundary:** only these two verify inbound signatures. The
WS/long-poll channels trust the platform-authenticated connection — if the
platform account or token is compromised, forged inbound is possible. Weixin
personal ilink has no per-message signature at all.

## 3. Replay, dedup, rate limits

- **ReplayGuard** — webhook channels reject messages outside a ±300 s
  freshness window and cache seen `channel|nonce` pairs. (WS/long-poll
  channels pass through — platform ordering applies.)
- **DedupStore** — SQLite `(channel, message_id)` `INSERT OR IGNORE`; 7-day
  TTL with a sweep every 1024 inserts.
- **RateLimiter** — 60 msg/min per channel + 10 msg/min per scope.

## 4. Who may talk to the agent: `allowFrom`

Per-channel sender whitelist (`outbound.rs`):

| Value | Behavior |
|---|---|
| unset or `*` | **open** — anyone who can reach the bot talks to the agent |
| `""` (empty string) | **fail-closed** — channel disabled |
| comma-separated sender IDs | only those senders |

`require_mention` (default **true**) additionally requires @-mentioning the
bot in group contexts. Unauthorized senders get a "not on allow_from list"
rejection.

**Before you expose a bridge, set explicit sender IDs.** The open default is
convenient for a first smoke test and unsafe beyond it.

## 5. Remote approvals (yolo)

- IM-granted yolo has a **3600 s TTL**, lives in memory only, is never written
  to disk, and dies on restart (`DEFAULT_APPROVAL_TTL_SECS`, engine + bridge
  wiring).
- The `allow_remote_yolo` master gate must be on for remote approval at all.
- Anti-replay for approvals rides on §3 (AC-8.4, shipped 2026-07-31).

## 6. Credentials

Bot tokens live in the OS secure store under the `remote` namespace,
referenced on disk as `keychain:v1:remote:<key>` — see
[Credential management](./credential-management.md). Rotate tokens on any
suspicion of leak.

## 7. Recommendations

1. Set explicit `allowFrom` sender IDs per channel (§4).
2. Keep webhook binds on loopback; terminate TLS at a proxy in front.
3. Prefer official-tier channels; treat the rest as best-effort.
4. Least-privilege bot scopes on each platform; enable MFA on the platform
   accounts themselves (that side is the platform's, not Desktop's).
5. Keep `allow_remote_yolo` off unless you actively need IM approvals;
   remember the 1 h TTL.

## 8. Honest boundaries

- Signature verification exists only for LINE + WeCom webhook (§2).
- Weixin personal ilink: no per-message signature; conditional support tier.
- Rate limits are in-memory — they reset on restart.
- The 14 implemented adapters ≠ the 10-channel official support tier.

## 9. File index

| Area | File |
|---|---|
| Channel adapters | `src-tauri/src/remote_im/channels/*.rs` |
| LINE / WeCom webhook + signatures | `src-tauri/src/remote_im/channels/line.rs`, `wecom.rs` |
| Replay guard | `src-tauri/src/remote_im/replay_guard.rs` |
| Dedup | `src-tauri/src/remote_im/dedup_store.rs` |
| Rate limiter | `src-tauri/src/remote_im/rate_limiter.rs` |
| allowFrom / mention gating | `src-tauri/src/remote_im/outbound.rs` |
| Approval TTL | `src-tauri/src/remote_im/engine.rs`, `bridge.rs` |
