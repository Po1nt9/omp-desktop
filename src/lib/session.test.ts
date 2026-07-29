import { describe, expect, it } from "vitest";
import {
  applyContextCompact,
  applyGeneratedImage,
  applyInterjection,
  applyStreamChunk,
  applyToolEvent,
  applyTurnError,
  buildSegmentsFromLegacy,
  canSend,
  canStop,
  canType,
  clearPriorTurnStreaming,
  compactMessageSegments,
  errorCopy,
  formatTurnErrorBody,
  isFailedToolStepMessage,
  messageSegments,
  splitThoughtPhases,
  isSessionBusy,
  isSessionLiveStreaming,
  isSessionNotLiveError,
  parseCompactContent,
  parseToolStepContent,
  pickLatestTurnTool,
  pickRunningTurnTool,
  toolStepDisplayTitle,
  preferSessionMessages,
  presentErrorBanner,
  snapshotOutgoingMessages,
  mergeSessionMessagesById,
  reconcileOptimisticDuplicates,
  isClientOptimisticId,
  weaveToolsIntoAssistantSegments,
  stripAnsi,
  truncateBeforeLastUser,
  truncateThroughUserPrompt,
  endIndexThroughUserPrompt,
  canRewindToUserPrompt,
  userPromptIndexOf,
  countUserPrompts,
  localRewindPoints,
  forkMessages,
  forkSessionTitle,
  type ChatMessage,
  type StreamPayload,
} from "./session";

describe("session projection", () => {
  it("input matrix Ready / Streaming / Stop (draft ok while stream; send blocked)", () => {
    expect(canType("ready")).toBe(true);
    expect(canType("idle")).toBe(true);
    // Draft allowed while streaming so the box is never "stuck" on pauses.
    expect(canType("streaming")).toBe(true);
    expect(canType("awaiting_permission")).toBe(false);
    expect(canSend("ready")).toBe(true);
    expect(canSend("idle")).toBe(true);
    expect(canStop("ready")).toBe(false);
    expect(canStop("streaming")).toBe(true);
    expect(canSend("streaming")).toBe(false);
  });

  it("isSessionBusy covers connect / stream / permission", () => {
    expect(isSessionBusy("idle")).toBe(false);
    expect(isSessionBusy("ready")).toBe(false);
    expect(isSessionBusy("disconnected")).toBe(false);
    expect(isSessionBusy("connecting")).toBe(true);
    expect(isSessionBusy("streaming")).toBe(true);
    expect(isSessionBusy("awaiting_permission")).toBe(true);
  });

  it("isSessionLiveStreaming excludes connecting (sidebar spinner silent)", () => {
    expect(isSessionLiveStreaming("connecting")).toBe(false);
    expect(isSessionLiveStreaming("idle")).toBe(false);
    expect(isSessionLiveStreaming("ready")).toBe(false);
    expect(isSessionLiveStreaming("streaming")).toBe(true);
    expect(isSessionLiveStreaming("awaiting_permission")).toBe(true);
  });

  it("isSessionNotLiveError only matches Host's targeted-send refusal", () => {
    // Host string form (tauri invoke rejects with the message).
    expect(
      isSessionNotLiveError(
        "CONNECT_FAILED: chat abc has no live agent process — reconnect and retry",
      ),
    ).toBe(true);
    expect(
      isSessionNotLiveError(
        new Error("CONNECT_FAILED: chat abc lost focus before send — retry"),
      ),
    ).toBe(true);
    // Mirror RPC error object shape.
    expect(
      isSessionNotLiveError({
        code: "HOST_ERROR",
        message: "CONNECT_FAILED: chat abc has no live agent process",
      }),
    ).toBe(true);
    // Other connect failures must NOT trigger the send retry loop.
    expect(
      isSessionNotLiveError("CONNECT_FAILED: handshake timed out"),
    ).toBe(false);
    expect(isSessionNotLiveError("PROCESS_LIMIT: pool full")).toBe(false);
    expect(isSessionNotLiveError(null)).toBe(false);
    expect(isSessionNotLiveError(undefined)).toBe(false);
  });

  it("truncateBeforeLastUser drops last user turn and everything after", () => {
    const msgs: ChatMessage[] = [
      { id: "u1", role: "user", content: "first" },
      { id: "a1", role: "assistant", content: "ok" },
      { id: "u2", role: "user", content: "second" },
      { id: "a2", role: "assistant", content: "fail", isError: true },
    ];
    expect(truncateBeforeLastUser(msgs)).toEqual([
      { id: "u1", role: "user", content: "first" },
      { id: "a1", role: "assistant", content: "ok" },
    ]);
    expect(
      truncateBeforeLastUser([{ id: "u1", role: "user", content: "only" }]),
    ).toEqual([]);
    expect(truncateBeforeLastUser([])).toEqual([]);
  });

  it("truncateThroughUserPrompt keeps the selected turn (ACP rewind semantics)", () => {
    const msgs: ChatMessage[] = [
      { id: "u1", role: "user", content: "first" },
      { id: "t1", role: "tool", content: "tool", marker: "tool_step" },
      { id: "a1", role: "assistant", content: "ok" },
      { id: "u2", role: "user", content: "second" },
      { id: "a2", role: "assistant", content: "later" },
    ];
    expect(truncateThroughUserPrompt(msgs, 0).map((m) => m.id)).toEqual([
      "u1",
      "t1",
      "a1",
    ]);
    expect(truncateThroughUserPrompt(msgs, 1).map((m) => m.id)).toEqual([
      "u1",
      "t1",
      "a1",
      "u2",
      "a2",
    ]);
    expect(truncateThroughUserPrompt(msgs, 2)).toEqual([]);
    expect(endIndexThroughUserPrompt(msgs, 0)).toBe(3);
    expect(canRewindToUserPrompt(msgs, 0)).toBe(true);
    expect(canRewindToUserPrompt(msgs, 1)).toBe(false);
    expect(userPromptIndexOf(msgs, "u2")).toBe(1);
    expect(userPromptIndexOf(msgs, "a1")).toBe(-1);
    expect(countUserPrompts(msgs)).toBe(2);
  });

  it("keeps interjections inside the surrounding rewind turn", () => {
    const messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "first" },
      { id: "a1", role: "assistant", content: "working" },
      {
        id: "i1",
        role: "user",
        content: "steer",
        marker: "interjection",
      },
      { id: "u2", role: "user", content: "next" },
    ];

    expect(countUserPrompts(messages)).toBe(2);
    expect(userPromptIndexOf(messages, "i1")).toBe(-1);
    expect(endIndexThroughUserPrompt(messages, 0)).toBe(3);
  });

  it("starts a new assistant row after a mid-turn interjection", () => {
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "build it" },
      {
        id: "a1",
        role: "assistant",
        content: "Working",
        streaming: true,
      },
    ];

    messages = applyInterjection(messages, {
      id: "i1",
      role: "user",
      content: "Use the existing component",
      marker: "interjection",
    });

    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a2",
      text: " on it",
      done: false,
      kind: "assistant",
    });

    expect(messages.map((message) => message.id)).toEqual([
      "u1",
      "a1",
      "i1",
      "a2",
    ]);
    expect(messages[1]).toMatchObject({
      id: "a1",
      content: "Working",
      streaming: false,
    });
    expect(messages[3]).toMatchObject({
      id: "a2",
      content: " on it",
      streaming: true,
    });
  });

  it("drops an empty optimistic assistant when interjected before output", () => {
    const messages = applyInterjection(
      [
        { id: "u1", role: "user", content: "build it" },
        {
          id: "a-pending-1",
          role: "assistant",
          content: "",
          streaming: true,
        },
      ],
      {
        id: "i1",
        role: "user",
        content: "Use the existing component",
        marker: "interjection",
      },
    );

    expect(messages.map((message) => message.id)).toEqual(["u1", "i1"]);
  });

    it("localRewindPoints lists one entry per user prompt", () => {
    const msgs: ChatMessage[] = [
      { id: "u1", role: "user", content: "  hello   world  " },
      { id: "a1", role: "assistant", content: "ok" },
      { id: "u2", role: "user", content: "x".repeat(100) },
    ];
    const pts = localRewindPoints(msgs, { previewMax: 10 });
    expect(pts).toEqual([
      { promptIndex: 0, messageId: "u1", preview: "hello wor…" },
      {
        promptIndex: 1,
        messageId: "u2",
        preview: "xxxxxxxxx…",
      },
    ]);
  });

  it("forkMessages copies through a turn and remaps ids", () => {
    const msgs: ChatMessage[] = [
      { id: "u1", role: "user", content: "first", streaming: true },
      { id: "a1", role: "assistant", content: "ok" },
      { id: "u2", role: "user", content: "second" },
    ];
    const forked = forkMessages(msgs, {
      throughUserPromptIndex: 0,
      idPrefix: "f",
    });
    expect(forked).toHaveLength(2);
    expect(forked[0].id).toMatch(/^f-0-u1$/);
    expect(forked[0].streaming).toBe(false);
    expect(forked[0].content).toBe("first");
    expect(forked[1].id).toMatch(/^f-1-a1$/);
    const full = forkMessages(msgs, { remapIds: false });
    expect(full.map((m) => m.id)).toEqual(["u1", "a1", "u2"]);
  });

  it("forkSessionTitle prefixes once", () => {
    expect(forkSessionTitle("My chat")).toBe("Fork of My chat");
    expect(forkSessionTitle("Fork of My chat")).toBe("Fork of My chat");
    expect(forkSessionTitle("")).toBe("Fork of chat");
  });

  it("preferSessionMessages keeps optimistic / streaming cache over disk", () => {
    const stored: ChatMessage[] = [
      { id: "u1", role: "user", content: "old" },
    ];
    const cached: ChatMessage[] = [
      { id: "u1", role: "user", content: "hello" },
      { id: "a1", role: "assistant", content: "partial", streaming: true },
    ];
    // Streaming cache kept, but disk-only rows still merge in
    const mergedStream = preferSessionMessages(cached, stored);
    expect(mergedStream.some((m) => m.streaming)).toBe(true);
    expect(preferSessionMessages(undefined, stored)).toEqual(stored);
    expect(preferSessionMessages([], stored)).toEqual(stored);
    // Equal length, disk has more text → prefer disk base
    const doneCache: ChatMessage[] = [
      { id: "u1", role: "user", content: "hi" },
      { id: "a1", role: "assistant", content: "ok" },
    ];
    const doneStore: ChatMessage[] = [
      { id: "u1", role: "user", content: "hi" },
      { id: "a1", role: "assistant", content: "ok full" },
    ];
    const done = preferSessionMessages(doneCache, doneStore);
    expect(done.find((m) => m.id === "a1")?.content).toBe("ok full");
  });

  it("preferSessionMessages merges Remote IM disk rows into cache", () => {
    const cached: ChatMessage[] = [
      { id: "u1", role: "user", content: "hi", createdAt: "2026-07-24T00:00:00Z" },
      { id: "a1", role: "assistant", content: "yo", createdAt: "2026-07-24T00:00:01Z" },
    ];
    const stored: ChatMessage[] = [
      ...cached,
      {
        id: "u-im",
        role: "user",
        content: "[Remote IM · weixin]\n继续",
        createdAt: "2026-07-25T00:00:00Z",
      },
      {
        id: "a-im",
        role: "assistant",
        content: "好的",
        createdAt: "2026-07-25T00:00:01Z",
      },
    ];
    const out = preferSessionMessages(cached, stored);
    expect(out.map((m) => m.id)).toEqual(["u1", "a1", "u-im", "a-im"]);
  });

  it("preferSessionMessages drops optimistic user when host UUID already has same body", () => {
    // After turn completes: cache still has u-${ts}, disk has host UUID.
    // Switch away → switch back must not append the first user bubble again.
    const cached: ChatMessage[] = [
      {
        id: "u-1710000000000",
        role: "user",
        content: "找一下奇妙森林这个项目有什么内容",
      },
      {
        id: "a1",
        role: "assistant",
        content: "概览……",
        segments: [
          { kind: "thought", text: "plan" },
          {
            kind: "tool",
            toolCallId: "t1",
            title: "Read",
            status: "completed",
          },
          { kind: "content", text: "概览……" },
        ],
      },
    ];
    const stored: ChatMessage[] = [
      {
        id: "6749cf2f-57b2-4576-b940-60957e43cd44",
        role: "user",
        content: "找一下奇妙森林这个项目有什么内容",
      },
      {
        id: "840227fd-3a82-4432-a829-49c18aa61327",
        role: "assistant",
        content: "概览……",
      },
      {
        id: "tool-t1",
        role: "tool",
        content: "Read x",
        marker: "tool_step",
        toolCallId: "t1",
        toolStatus: "completed",
      },
    ];
    const out = preferSessionMessages(cached, stored);
    const users = out.filter((m) => m.role === "user");
    expect(users).toHaveLength(1);
    expect(isClientOptimisticId(users[0]!.id)).toBe(false);
    expect(out[out.length - 1]!.role).not.toBe("user");
    // User stays at the head (in-place replace), not moved to the tail.
    expect(out[0]!.role).toBe("user");
  });

  it("reconcileOptimisticDuplicates replaces u-${ts} in place (not tail)", () => {
    const msgs: ChatMessage[] = [
      {
        id: "u-1710000000001",
        role: "user",
        content: "hello",
      },
      { id: "uuid-asst", role: "assistant", content: "hi" },
      {
        id: "uuid-user",
        role: "user",
        content: "hello",
      },
    ];
    const out = reconcileOptimisticDuplicates(msgs);
    expect(out.map((m) => m.id)).toEqual(["uuid-user", "uuid-asst"]);
    expect(out[0]!.role).toBe("user");
  });

  it("applyStreamChunk grows assistant text once per chunk", () => {
    let messages: ChatMessage[] = [];
    const chunks: StreamPayload[] = [
      { sessionId: "s", messageId: "m1", text: "Hel", done: false, kind: "assistant" },
      { sessionId: "s", messageId: "m1", text: "lo", done: false, kind: "assistant" },
      { sessionId: "s", messageId: "m1", text: "", done: true, kind: "assistant" },
    ];
    for (const c of chunks) messages = applyStreamChunk(messages, c);
    expect(messages).toHaveLength(1);
    expect(messages[0]!.content).toBe("Hello");
    expect(messages[0]!.streaming).toBe(false);
  });

  it("does not double-append when same sequence applied once", () => {
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "hi" },
    ];
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "直接",
      done: false,
      kind: "assistant",
    });
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "干活",
      done: true,
      kind: "assistant",
    });
    expect(messages.find((m) => m.role === "assistant")!.content).toBe("直接干活");
  });

  it("splitThoughtPhases separates multi-phase markers", () => {
    expect(splitThoughtPhases("a\n\n⟪phase⟫\n\nb")).toEqual(["a", "b"]);
    expect(splitThoughtPhases("only")).toEqual(["only"]);
  });

  it("isFailedToolStepMessage detects failed tools only", () => {
    expect(
      isFailedToolStepMessage({
        id: "tool-a",
        role: "tool",
        content: "Read x",
        marker: "tool_step",
        toolStatus: "completed",
      }),
    ).toBe(false);
    expect(
      isFailedToolStepMessage({
        id: "tool-b",
        role: "tool",
        content: "Bash",
        marker: "tool_step",
        toolStatus: "failed",
        isError: true,
      }),
    ).toBe(true);
  });

  it("spurious new-phase without body merges into one thought (no 思考 2)", () => {
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "hi" },
      {
        id: "a1",
        role: "assistant",
        content: "",
        thought: "first",
        thoughtPhases: ["first"],
        segments: [{ kind: "thought", text: "first" }],
        streaming: true,
      },
    ];
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "second",
      done: false,
      kind: "thought",
      thoughtPhase: "new",
    });
    // Adjacent thoughts must not become multiple UI rows.
    expect(messages[1]!.segments).toEqual([
      { kind: "thought", text: "firstsecond" },
    ]);
    expect(messages[1]!.thoughtPhases).toEqual(["firstsecond"]);
  });

  it("buildSegmentsFromLegacy stacks multi-phase thought before body", () => {
    const segs = buildSegmentsFromLegacy(
      "answer body",
      "a\n\n⟪phase⟫\n\nb\n\n⟪phase⟫\n\nc",
      undefined,
    );
    // One thought block + body — never "body then 思考 2 / 3".
    expect(segs).toEqual([
      { kind: "thought", text: "a\n\nb\n\nc" },
      { kind: "content", text: "answer body" },
    ]);
  });

  it("compactMessageSegments merges adjacent thoughts", () => {
    expect(
      compactMessageSegments([
        { kind: "thought", text: "a" },
        { kind: "thought", text: "b" },
        { kind: "content", text: "hi" },
        { kind: "thought", text: "c" },
        { kind: "thought", text: "" },
      ]),
    ).toEqual([
      { kind: "thought", text: "a\n\nb" },
      { kind: "content", text: "hi" },
      { kind: "thought", text: "c" },
    ]);
  });

  it("compactMessageSegments keeps tools and coalesces same toolCallId", () => {
    const segs = compactMessageSegments([
      { kind: "thought", text: "t" },
      {
        kind: "tool",
        toolCallId: "x",
        title: "Read a",
        status: "running",
        streaming: true,
      },
      {
        kind: "tool",
        toolCallId: "x",
        title: "Read a",
        status: "completed",
        streaming: false,
      },
      { kind: "content", text: "done" },
    ]);
    expect(segs.map((s) => s.kind)).toEqual(["thought", "tool", "content"]);
    expect(segs[1]).toMatchObject({
      kind: "tool",
      toolCallId: "x",
      status: "completed",
      streaming: false,
    });
  });

  it("messageSegments compacts live multi thought rows", () => {
    const segs = messageSegments({
      id: "a1",
      role: "assistant",
      content: "done",
      segments: [
        { kind: "thought", text: "p1" },
        { kind: "thought", text: "p2" },
        { kind: "content", text: "done" },
        { kind: "thought", text: "p3" },
      ],
    });
    expect(segs).toEqual([
      { kind: "thought", text: "p1\n\np2" },
      { kind: "content", text: "done" },
      { kind: "thought", text: "p3" },
    ]);
  });

  it("weaveToolsIntoAssistantSegments puts journal tools between thought and content", () => {
    // Host journal shape: U → A (final) → tools (tools ran mid-turn).
    const woven = weaveToolsIntoAssistantSegments([
      { id: "u1", role: "user", content: "q" },
      {
        id: "a1",
        role: "assistant",
        content: "answer",
        createdAt: "2026-07-26T01:11:32Z",
        segments: [
          { kind: "thought", text: "why" },
          { kind: "content", text: "answer" },
        ],
      },
      {
        id: "tool-t1",
        role: "tool",
        content: "Read x",
        marker: "tool_step",
        toolCallId: "t1",
        toolKind: "read_file",
        toolStatus: "completed",
        toolPath: "/x.ts",
        createdAt: "2026-07-26T01:10:47Z",
      },
      {
        id: "tool-t2",
        role: "tool",
        content: "Edit y",
        marker: "tool_step",
        toolCallId: "t2",
        toolKind: "search_replace",
        toolStatus: "failed",
        isError: true,
        createdAt: "2026-07-26T01:10:58Z",
      },
    ]);
    const segs = messageSegments(woven[1]!);
    // History reconstruction: thought → tools → content (not tools under the answer).
    expect(segs.map((s) => s.kind)).toEqual([
      "thought",
      "tool",
      "tool",
      "content",
    ]);
    expect(segs[2]).toMatchObject({
      kind: "tool",
      toolCallId: "t2",
      isError: true,
    });
  });

  it("weaveToolsIntoAssistantSegments attaches tools that appear before assistant in array", () => {
    // Broken createdAt-sort shape: U → tools → A
    const woven = weaveToolsIntoAssistantSegments([
      { id: "u1", role: "user", content: "q" },
      {
        id: "tool-t1",
        role: "tool",
        content: "Read x",
        marker: "tool_step",
        toolCallId: "t1",
        toolKind: "read_file",
        toolStatus: "completed",
      },
      {
        id: "tool-t2",
        role: "tool",
        content: "Read y",
        marker: "tool_step",
        toolCallId: "t2",
        toolKind: "read_file",
        toolStatus: "completed",
      },
      {
        id: "a1",
        role: "assistant",
        content: "answer",
        thought: "plan",
        segments: [
          { kind: "thought", text: "plan" },
          { kind: "content", text: "answer" },
        ],
      },
    ]);
    const segs = messageSegments(woven.find((m) => m.id === "a1")!);
    expect(segs.map((s) => s.kind)).toEqual([
      "thought",
      "tool",
      "tool",
      "content",
    ]);
  });

  it("mergeSessionMessagesById keeps journal order (no createdAt re-sort)", () => {
    const primary: ChatMessage[] = [
      {
        id: "u1",
        role: "user",
        content: "q",
        createdAt: "2026-07-26T01:10:41Z",
      },
      {
        id: "a1",
        role: "assistant",
        content: "answer",
        createdAt: "2026-07-26T01:11:32Z",
      },
      {
        id: "tool-t1",
        role: "tool",
        content: "Read",
        marker: "tool_step",
        createdAt: "2026-07-26T01:10:47Z",
      },
    ];
    const merged = mergeSessionMessagesById(primary, []);
    expect(merged.map((m) => m.id)).toEqual(["u1", "a1", "tool-t1"]);
  });

  it("places journal-only rows at their turn position, not at the tail", () => {
    // Regression: a mid-turn session switch can leave the cache holding only
    // the streaming assistant. Appending disk-only rows rendered the user's
    // own prompt *after* the finished answer.
    const cached: ChatMessage[] = [
      { id: "a-host", role: "assistant", content: "…answer…", streaming: true },
    ];
    const stored: ChatMessage[] = [
      { id: "u-host", role: "user", content: "查看项目内的内容" },
      { id: "a-host", role: "assistant", content: "…answer…" },
      { id: "tool-1", role: "tool", content: "tool_step|completed", marker: "tool_step" },
      { id: "tool-2", role: "tool", content: "tool_step|completed", marker: "tool_step" },
    ];
    expect(mergeSessionMessagesById(cached, stored).map((m) => m.id)).toEqual([
      "u-host",
      "a-host",
      "tool-1",
      "tool-2",
    ]);
    // Same through the real entry point the workbench uses.
    expect(preferSessionMessages(cached, stored).map((m) => m.role)).toEqual([
      "user",
      "assistant",
      "tool",
      "tool",
    ]);
  });

  it("snapshotOutgoingMessages never clobbers a populated cache with an empty view", () => {
    const cached: ChatMessage[] = [
      { id: "u1", role: "user", content: "q" },
      { id: "a1", role: "assistant", content: "a" },
    ];
    // Workbench already cleared (user hit "new chat") — keep the real turn.
    expect(snapshotOutgoingMessages(cached, [])).toEqual(cached);
    // Normal case: the viewed thread is authoritative.
    const viewed: ChatMessage[] = [{ id: "u2", role: "user", content: "q2" }];
    expect(snapshotOutgoingMessages(cached, viewed)).toEqual(viewed);
    // Nothing anywhere → empty.
    expect(snapshotOutgoingMessages(undefined, [])).toEqual([]);
  });

  it("keeps repeated journal ids (tool_step rows share call ids)", () => {
    const primary: ChatMessage[] = [
      { id: "u1", role: "user", content: "q" },
      { id: "tool-call-a", role: "tool", content: "s1", marker: "tool_step" },
      { id: "tool-call-a", role: "tool", content: "s2", marker: "tool_step" },
    ];
    expect(mergeSessionMessagesById(primary, []).map((m) => m.id)).toEqual([
      "u1",
      "tool-call-a",
      "tool-call-a",
    ]);
  });

  it("interleaves several journal-only rows before their shared anchor", () => {
    const cached: ChatMessage[] = [{ id: "a1", role: "assistant", content: "x" }];
    const stored: ChatMessage[] = [
      { id: "u1", role: "user", content: "q1" },
      { id: "t1", role: "tool", content: "one", marker: "tool_step" },
      { id: "a1", role: "assistant", content: "x" },
      { id: "t2", role: "tool", content: "two", marker: "tool_step" },
    ];
    expect(mergeSessionMessagesById(cached, stored).map((m) => m.id)).toEqual([
      "u1",
      "t1",
      "a1",
      "t2",
    ]);
  });

  it("interleaves thought and content in stream order", () => {
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "hi" },
      { id: "a1", role: "assistant", content: "", streaming: true },
    ];
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "think1",
      done: false,
      kind: "thought",
      thoughtPhase: "open",
    });
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "hello ",
      done: false,
      kind: "assistant",
    });
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "think2",
      done: false,
      kind: "thought",
      thoughtPhase: "new",
    });
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "world",
      done: false,
      kind: "assistant",
    });
    const a = messages[1]!;
    expect(a.segments).toEqual([
      { kind: "thought", text: "think1" },
      { kind: "content", text: "hello " },
      { kind: "thought", text: "think2" },
      { kind: "content", text: "world" },
    ]);
    expect(a.content).toBe("hello world");
    expect(a.thoughtPhases).toEqual(["think1", "think2"]);
  });

  it("stream chunks never append onto prior-turn assistants", () => {
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "first" },
      {
        id: "a1",
        role: "assistant",
        content: "old answer",
        streaming: true, // stuck flag from missed done
      },
      { id: "u2", role: "user", content: "second" },
      { id: "a-pending-1", role: "assistant", content: "", streaming: true },
    ];
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a2",
      text: "new answer",
      done: false,
      kind: "assistant",
    });
    expect(messages.find((m) => m.id === "a1")!.content).toBe("old answer");
    const current = messages.find(
      (m) => m.id === "a2" || m.id === "a-pending-1",
    )!;
    expect(current.content).toBe("new answer");
    expect(current.id).toBe("a2"); // adopted host id
  });

  it("clearPriorTurnStreaming only clears assistants before last user", () => {
    const msgs: ChatMessage[] = [
      { id: "a0", role: "assistant", content: "x", streaming: true },
      { id: "u1", role: "user", content: "hi" },
      { id: "a1", role: "assistant", content: "", streaming: true },
    ];
    const next = clearPriorTurnStreaming(msgs);
    expect(next[0]!.streaming).toBe(false);
    expect(next[2]!.streaming).toBe(true);
  });

  it("next-send optimistic path does not leave prior turn streaming (no re-type history)", () => {
    // Simulate turn 1 finished (done chunk) then user sends turn 2.
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "first" },
      {
        id: "a1",
        role: "assistant",
        content: "answer one",
        streaming: true,
      },
    ];
    messages = applyStreamChunk(messages, {
      sessionId: "s",
      messageId: "a1",
      text: "",
      done: true,
      kind: "assistant",
    });
    expect(messages[1]!.streaming).toBe(false);
    expect(messages[1]!.content).toBe("answer one");

    // Same path as executeSend appendOptimistic: clear prior streaming flags
    // then append new user + pending assistant — prior content stays put once.
    const cleaned = clearPriorTurnStreaming(messages);
    const nextSend: ChatMessage[] = [
      ...cleaned,
      { id: "u2", role: "user", content: "second" },
      { id: "a-pending-2", role: "assistant", content: "", streaming: true },
    ];
    expect(nextSend.filter((m) => m.role === "assistant" && m.streaming)).toHaveLength(
      1,
    );
    expect(nextSend[1]!.content).toBe("answer one");
    expect(nextSend[1]!.streaming).toBe(false);
  });

  it("errorCopy distinguishes seven codes (English default)", () => {
    expect(errorCopy("CLI_NOT_FOUND")).toMatch(/CLI/i);
    expect(errorCopy("AUTH_FAILED")).toMatch(/Auth|sign.?in|credential/i);
    expect(errorCopy("NETWORK_PROVIDER")).toMatch(/Network|model|provider/i);
    expect(errorCopy("AGENT_CRASHED")).toMatch(/crash|process|agent/i);
    expect(errorCopy("QUOTA_EXCEEDED")).toMatch(/Quota|limit|usage/i);
    expect(errorCopy("CONNECT_FAILED")).toMatch(/connect/i);
    expect(errorCopy("PROCESS_LIMIT")).toMatch(/limit|process|concurrent/i);
  });

  it("formatTurnErrorBody maps connect / quota phrases", () => {
    expect(
      formatTurnErrorBody(
        {
          content:
            "Could not connect the agent for this session; edit aborted.",
        },
        "en",
      ),
    ).toMatch(/connect/i);
    expect(
      formatTurnErrorBody({ content: "rate limit exceeded (429)" }, "en"),
    ).toMatch(/quota|rate/i);
  });

  it("presentErrorBanner shows friendly deck without MCP dumps", () => {
    const raw =
      'rpc timeout on session/prompt (id=4) after 600s; stderr: ...\nERROR worker quit with fatal: Connection refused';
    const fromAgent = presentErrorBanner(
      { code: "NETWORK_PROVIDER", message: raw },
      null,
      "en",
    );
    expect(fromAgent?.summary).toMatch(/timed?\s*out|timeout|network|model|provider/i);
    expect(fromAgent?.cause).toBeTruthy();
    expect(fromAgent?.summary).not.toMatch(/Connection refused/);
    expect(fromAgent?.summary).not.toMatch(/stderr/i);
    expect(fromAgent?.detail).toBeNull();
    expect(fromAgent?.primary?.id).toBeTruthy();
    expect(fromAgent?.reconnectHint).toBe(true);

    const fromLocal = presentErrorBanner(
      null,
      `NETWORK_PROVIDER: ${raw}`,
      "en",
    );
    expect(fromLocal?.code).toBe("NETWORK_PROVIDER");
    expect(fromLocal?.summary).toMatch(/timed?\s*out|timeout|network|model|provider/i);
    expect(fromLocal?.detail).toBeNull();
    expect(fromLocal?.primary?.label.length).toBeGreaterThan(0);

    const short = presentErrorBanner(null, "Select a project first", "en");
    expect(short?.summary).toBe("Select a project first");
    expect(short?.detail).toBeNull();
    expect(short?.primary?.id).toBe("dismiss");
  });

  it("presentErrorBanner decks the four product classes", () => {
    const cli = presentErrorBanner(
      { code: "CLI_NOT_FOUND", message: "missing" },
      null,
      "en",
    );
    expect(cli?.primary?.id).toBe("open_doctor");
    expect(cli?.secondary?.id).toBe("open_runtime");

    const auth = presentErrorBanner(
      { code: "AUTH_FAILED", message: "401" },
      null,
      "en",
    );
    expect(auth?.primary?.id).toBe("open_account");

    const crash = presentErrorBanner(
      { code: "AGENT_CRASHED", message: "exit 1" },
      null,
      "en",
    );
    expect(crash?.primary?.id).toBe("reconnect");
  });

  it("presentErrorBanner routes CLI_TOO_OLD to the upgrade deck", () => {
    const fromAgent = presentErrorBanner(
      {
        code: "CLI_TOO_OLD",
        message: "runtime 0.2.101 is older than the required 0.2.112",
      },
      null,
      "en",
    );
    expect(fromAgent?.code).toBe("CLI_TOO_OLD");
    expect(fromAgent?.primary?.id).toBe("upgrade_cli");

    // From the launch-time probe (coded localError string).
    const fromLocal = presentErrorBanner(
      null,
      "CLI_TOO_OLD: runtime 0.2.101 < required 0.2.112",
      "en",
    );
    expect(fromLocal?.code).toBe("CLI_TOO_OLD");
    expect(fromLocal?.primary?.id).toBe("upgrade_cli");
    expect(fromLocal?.summary.toLowerCase()).toMatch(/runtime/);
  });

  it("formatTurnErrorBody maps turn_timeout tag", () => {
    const body = formatTurnErrorBody(
      {
        code: "NETWORK_PROVIDER",
        message: "turn_timeout",
        content: "**NETWORK_PROVIDER**\n\nturn_timeout",
      },
      "en",
    );
    expect(body).toMatch(/timed?\s*out|timeout/i);
    expect(body).not.toMatch(/NETWORK_PROVIDER|rpc timeout|stderr/i);
  });

  it("stripAnsi removes SGR sequences", () => {
    expect(stripAnsi("\u001b[31mERROR\u001b[0m boom")).toBe("ERROR boom");
  });

  it("applyTurnError replaces optimistic thinking with friendly error", () => {
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "hi" },
      { id: "a-pending", role: "assistant", content: "", streaming: true },
    ];
    messages = applyTurnError(
      messages,
      {
        messageId: "host-mid",
        code: "NETWORK_PROVIDER",
        message:
          'rpc timeout on session/prompt (id=6) after 600s; stderr: Connection refused',
        content:
          '**NETWORK_PROVIDER**\n\nrpc timeout on session/prompt (id=6) after 600s; stderr: Connection refused',
      },
      "en",
    );
    expect(messages).toHaveLength(2);
    const err = messages[1]!;
    expect(err.role).toBe("assistant");
    expect(err.isError).toBe(true);
    expect(err.streaming).toBe(false);
    expect(err.content).toMatch(/timed?\s*out|timeout/i);
    expect(err.content).not.toMatch(/Connection refused|stderr|rpc timeout/i);
  });

  it("applyGeneratedImage attaches to streaming assistant and dedupes", () => {
    let messages: ChatMessage[] = [
      { id: "u1", role: "user", content: "draw a cat" },
      { id: "a-pending", role: "assistant", content: "", streaming: true },
    ];
    messages = applyGeneratedImage(messages, {
      path: "/tmp/images/1.jpg",
      name: "1.jpg",
    });
    expect(messages[1]!.attachments).toEqual([
      { path: "/tmp/images/1.jpg", name: "1.jpg", isDir: false },
    ]);
    // second time same path → no dup
    messages = applyGeneratedImage(messages, {
      path: "/tmp/images/1.jpg",
      name: "1.jpg",
    });
    expect(messages[1]!.attachments).toHaveLength(1);
    messages = applyGeneratedImage(messages, {
      path: "/tmp/images/2.png",
    });
    expect(messages[1]!.attachments).toHaveLength(2);
    expect(messages[1]!.attachments![1]!.name).toBe("2.png");
  });
});

describe("context compact markers", () => {
  it("parseCompactContent reads host journal format", () => {
    const meta = parseCompactContent(
      "context_compact|auto|tokens:120000->40000\nkept auth design",
    );
    expect(meta?.trigger).toBe("auto");
    expect(meta?.tokensBefore).toBe(120000);
    expect(meta?.tokensAfter).toBe(40000);
    expect(meta?.summaryPreview).toBe("kept auth design");
  });

  it("applyContextCompact appends marker row", () => {
    const next = applyContextCompact([], {
      messageId: "c1",
      trigger: "auto",
      tokensBefore: 1000,
      tokensAfter: 400,
    });
    expect(next).toHaveLength(1);
    expect(next[0]?.marker).toBe("context_compact");
    expect(next[0]?.compactMeta?.tokensBefore).toBe(1000);
  });
});

describe("tool activity", () => {
  it("applyToolEvent upserts by toolCallId", () => {
    let m = applyToolEvent([], {
      toolCallId: "t1",
      title: "read_file",
      kind: "read",
      status: "in_progress",
      path: "/tmp/a.ts",
    });
    expect(m).toHaveLength(1);
    expect(m[0]?.streaming).toBe(true);
    m = applyToolEvent(m, {
      toolCallId: "t1",
      title: "Read /tmp/a.ts",
      kind: "read",
      status: "completed",
      path: "/tmp/a.ts",
    });
    expect(m).toHaveLength(1);
    expect(m[0]?.streaming).toBe(false);
    expect(m[0]?.content).toContain("Read");
  });

  it("parseToolStepContent", () => {
    const p = parseToolStepContent(
      "tool_step|completed|read|Read foo\n/tmp/foo",
    );
    expect(p?.status).toBe("completed");
    expect(p?.title).toBe("Read foo");
  });

  it("pickLatestTurnTool prefers running tool in current turn", () => {
    let m = applyToolEvent(
      [
        {
          id: "u1",
          role: "user",
          content: "hi",
          createdAt: new Date().toISOString(),
        },
      ],
      {
        toolCallId: "t1",
        title: "Read a",
        kind: "read",
        status: "completed",
      },
    );
    m = applyToolEvent(m, {
      toolCallId: "t2",
      title: "Search b",
      kind: "search",
      status: "in_progress",
    });
    const latest = pickLatestTurnTool(m);
    expect(latest?.toolCallId).toBe("t2");
    expect(latest?.streaming).toBe(true);
  });

  it("pickRunningTurnTool only returns in-flight tool (hide when done)", () => {
    let m = applyToolEvent(
      [
        {
          id: "u1",
          role: "user",
          content: "hi",
          createdAt: new Date().toISOString(),
        },
      ],
      {
        toolCallId: "t1",
        title: "Listing files in private persona folder",
        kind: "list",
        status: "in_progress",
      },
    );
    expect(pickRunningTurnTool(m)?.content).toContain("Listing files");
    m = applyToolEvent(m, {
      toolCallId: "t1",
      title: "Listing files in private persona folder",
      kind: "list",
      status: "completed",
    });
    expect(pickRunningTurnTool(m)).toBeNull();
  });

  it("toolStepDisplayTitle prefers plain content title", () => {
    expect(
      toolStepDisplayTitle({
        id: "tool-1",
        role: "tool",
        content: "Listing files in private persona folder",
        marker: "tool_step",
      }),
    ).toBe("Listing files in private persona folder");
    expect(
      toolStepDisplayTitle({
        id: "tool-2",
        role: "tool",
        content: "tool_step|completed|read|Read foo",
        marker: "tool_step",
      }),
    ).toBe("Read foo");
  });

  it("never surfaces bare tool placeholder; prefers detail/path", () => {
    expect(
      toolStepDisplayTitle({
        id: "tool-3",
        role: "tool",
        content: "tool",
        toolDetail: "ls -la /tmp",
        marker: "tool_step",
      }),
    ).toBe("ls -la /tmp");
    expect(
      toolStepDisplayTitle({
        id: "tool-4",
        role: "tool",
        content: "tool",
        marker: "tool_step",
      }),
    ).toBe("");
    let m = applyToolEvent([], {
      toolCallId: "t-gen",
      title: "tool",
      kind: "tool",
      status: "in_progress",
    });
    expect(pickRunningTurnTool(m)).toBeNull();
    m = applyToolEvent(m, {
      toolCallId: "t-gen",
      title: "tool",
      kind: "bash",
      status: "in_progress",
      detail: "npm test",
    });
    expect(pickRunningTurnTool(m)?.content).toBe("npm test");
    // Don't downgrade a good title on a vague update
    m = applyToolEvent(m, {
      toolCallId: "t-gen",
      title: "tool",
      kind: "bash",
      status: "in_progress",
    });
    expect(m[0]?.content).toBe("npm test");
  });
});
