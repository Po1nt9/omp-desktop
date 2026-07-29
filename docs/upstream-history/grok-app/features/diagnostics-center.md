# Diagnostics center

Free-form host errors are classified into product decks (CLI / auth / network / crash) via `classifyErrorMessage` / `resolveErrorDeckCode`.

## Host file logs

On startup the Host enables dual-sink tracing:

| Sink | Location |
|------|----------|
| stderr | When launched from a terminal |
| Daily rolling file | `{app_data}/logs/app.log.YYYY-MM-DD` |

`RUST_LOG` still controls the filter (default `info`). Support bundles and Doctor can pick up the `logs/` directory after a mid-turn failure.

## Tool heartbeat (protocol)

While a turn has open tool call ids, Host emits (about every 25s):

```json
{
  "sessionId": "…",
  "toolCallIds": ["…"],
  "openCount": 1,
  "intervalSecs": 25
}
```

Event name: `session://tool_heartbeat`. Purpose: re-arm stream-stall progress and
give UI/diagnostics an explicit “tools still open” signal without requiring CLI
progress lines. Heartbeats stop if the oldest open tool exceeds 3 hours.
