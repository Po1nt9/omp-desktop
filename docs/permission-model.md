# Permission model

How OMP Desktop decides whether a tool call needs your approval — approval
modes, per-path decisions, fail-closed rules.

> **Current status (2026-07-31):** the permission ladder + path-aware
> auto-allow rules are implemented in `src-tauri/src/permission.rs` (35 tests)
> and wired to the session policy dropdown, the YOLO two-step confirm,
> per-request approval options from the Runtime, and the `[permission]` rules
> panel. Default mode is **Ask**: everything prompts unless a rule below says
> otherwise.

## 1. Approval modes

Settings / chat-header policy dropdown (`PERMISSION_POLICIES`):

| Mode | Auto-allows | Notes |
|---|---|---|
| `ask` (default) | nothing | every actionable tool call prompts |
| `accept_edits` | edit tools **except deletes** | the delete carve-out (§3) always still prompts |
| `allow_for_session` | approved tool+path, remembered for this session | session cache covers **in-project** paths only |
| `dont_ask` | suppresses prompts where policy permits | lower rank than `ask` — see ladder |
| `always_approve` | everything, including outside-project paths | **dangerous** — enabling requires the two-step in-app confirm |

Internal ranks:
`AlwaysApprove(5) > AllowForSession(4) > AcceptEdits(3) > Ask(2) = AllowOnce(2) > DontAsk(1) > Deny(0)`
(`permission.rs`). Higher wins when policies combine.

## 2. Per-path decisions

`may_auto_allow` decides per tool call, per path:

| Situation | Decision |
|---|---|
| Path **outside** the project root | only `always_approve` auto-allows; everything else prompts |
| Project root cannot be canonicalized | **fail closed** — prompt |
| Session-cached approval (from `allow_for_session`) | auto-allow only for paths inside the project |
| `accept_edits` + edit tool + in-project | auto-allow — unless the tool is a delete (§3) |
| Downloads into the project directory | auto-allow |
| Project marked untrusted | force `ask` regardless of mode |

## 3. The delete carve-out

Delete tools (`delete_file`, `rm`, `remove`, any tool id containing
"delete") **never** inherit edit approval — even under `accept_edits` they
prompt. Deletes are irreversible; they are their own risk class (§17.3).

## 4. Per-request approvals

Each pending approval dialog offers the options the Runtime (ACP) sent for
that request — typically **once / session / always** (`pick_option_id`).
Requests bind runtime/session/turn/request-id and die on timeout, restart, or
turn end; the first legal decision wins per pending request.

**Honest caveat:** there is no global "allow once" chip in the mode dropdown —
once/session/always are per-request choices supplied by the Runtime, not
modes you preset.

## 5. Subagents

A subagent's effective policy is the **narrower of** its own and its parent's
(`subagent_effective_policy`) — a child can never widen what the parent
disallowed.

## 6. YOLO mode

The YOLO toggle (`always_approve` for the session) requires a two-step
in-app confirm dialog (App `setAppDialog` — no `window.confirm`). Remote (IM)
yolo is separate: TTL-bound and memory-only — see
[Remote access risk](./remote-access-risk.md).

## 7. Persistent rules

Settings → Permission Rules panel edits `[permission]` `allow` / `deny` /
`ask` lists in the Runtime home `config.toml`. Explicit rules apply on top of
the mode ladder.

## 8. Honest boundaries

- No global `allow_once` chip (§4).
- The UI never fakes persistence the Runtime didn't grant — "session" scopes
  end with the session.
- Pending approvals do not survive a restart.

## 9. File index

| Area | File |
|---|---|
| Ladder + path rules | `src-tauri/src/permission.rs` |
| Policy dropdown options | `src/constants/modelOptions.ts` (`PERMISSION_POLICIES`) |
| Rules panel | `src/components/PermissionRulesPanel.tsx` |
| YOLO confirm + session policy wiring | `src/App.tsx` |
