import { describe, expect, it } from "vitest";
import {
  OmpDesktopV1Client,
  RUNTIME_UNAVAILABLE,
  UNKNOWN_METHOD,
  isDesktopV1Error,
  type DesktopV1Capability,
} from "./index";

describe("OmpDesktopV1Client", () => {
  it("returns runtime_unavailable when capability is not negotiated", async () => {
    const client = new OmpDesktopV1Client();
    expect(client.hasCapability).toBe(false);

    const result = await client.call("sessions.listAll", {});
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.code).toBe("runtime_unavailable");
      expect(result.error.messageKey).toBe("runtime.unavailable");
    }
  });

  it("rejects unknown methods even when capability is present", async () => {
    const client = new OmpDesktopV1Client();
    // Negotiate a capability that does NOT advertise `nonexistent.method`.
    // The client must reject the call with `unknown_method` instead of
    // attempting a real request.
    const cap: DesktopV1Capability = {
      schemaVersion: 1,
      schemaDigest: "test-digest",
      methods: ["_omp/desktop/v1/sessions.listAll"],
      notifications: [],
      optionalFeatures: [],
    };
    client.setCapability(cap);
    expect(client.hasCapability).toBe(true);

    // `nonexistent` is not a valid MethodName, so we cast for the test.
    // The runtime contract is: unknown short names return `unknown_method`.
    const result = await client.call(
      "nonexistent" as never,
      {} as never,
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.code).toBe("unknown_method");
      expect(result.error.messageKey).toBe("compat.unknownMethod");
    }
  });

  it("returns runtime_unavailable for advertised methods in Plan 2", async () => {
    // Even when the method IS in the capability list, Plan 2 has no real
    // transport wired, so the request must still fail with `runtime_unavailable`.
    // Plan 3 will inject the transport and make this succeed.
    const client = new OmpDesktopV1Client();
    const cap: DesktopV1Capability = {
      schemaVersion: 1,
      schemaDigest: "test-digest",
      methods: ["_omp/desktop/v1/sessions.listAll"],
      notifications: [],
      optionalFeatures: [],
    };
    client.setCapability(cap);

    const result = await client.call("sessions.listAll", { limit: 10 });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.code).toBe("runtime_unavailable");
    }
  });

  it("clearing capability restores fail-closed state", async () => {
    const client = new OmpDesktopV1Client();
    const cap: DesktopV1Capability = {
      schemaVersion: 1,
      schemaDigest: "abc",
      methods: ["_omp/desktop/v1/sessions.listAll"],
      notifications: [],
      optionalFeatures: [],
    };
    client.setCapability(cap);
    expect(client.hasCapability).toBe(true);

    client.setCapability(null);
    expect(client.hasCapability).toBe(false);

    const result = await client.call("sessions.listAll", {});
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.code).toBe("runtime_unavailable");
    }
  });
});

describe("error sentinels", () => {
  it("RUNTIME_UNAVAILABLE has stable code and messageKey", () => {
    expect(RUNTIME_UNAVAILABLE.code).toBe("runtime_unavailable");
    expect(RUNTIME_UNAVAILABLE.messageKey).toBe("runtime.unavailable");
    expect(RUNTIME_UNAVAILABLE.recoverable).toBe(false);
    expect(RUNTIME_UNAVAILABLE.retryable).toBe(false);
  });

  it("UNKNOWN_METHOD has stable code and messageKey", () => {
    expect(UNKNOWN_METHOD.code).toBe("unknown_method");
    expect(UNKNOWN_METHOD.messageKey).toBe("compat.unknownMethod");
    expect(UNKNOWN_METHOD.recoverable).toBe(false);
    expect(UNKNOWN_METHOD.retryable).toBe(false);
  });

  it("isDesktopV1Error narrows valid error shapes", () => {
    expect(isDesktopV1Error(RUNTIME_UNAVAILABLE)).toBe(true);
    expect(isDesktopV1Error(UNKNOWN_METHOD)).toBe(true);
    expect(isDesktopV1Error(null)).toBe(false);
    expect(isDesktopV1Error({})).toBe(false);
    expect(isDesktopV1Error({ code: "x" })).toBe(false); // missing messageKey
    expect(
      isDesktopV1Error({ code: "x", messageKey: "y" }),
    ).toBe(true);
  });
});

describe("transport seam (AC-1.5)", () => {
  const capAll: DesktopV1Capability = {
    schemaVersion: 1,
    schemaDigest: "test-digest",
    methods: [
      "_omp/desktop/v1/todo.list",
      "_omp/desktop/v1/subagents.status",
      "_omp/desktop/v1/subagents.setEnabled",
    ],
    notifications: [],
    optionalFeatures: [],
  };

  it("todo.list round-trips phases through the injected transport", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    const seen: Array<{ method: string; params: unknown }> = [];
    client.setTransport(async (method, params) => {
      seen.push({ method, params });
      return {
        phases: [
          {
            name: "phase-1",
            tasks: [
              { content: "design", status: "completed" },
              { content: "implement", status: "in_progress" },
              { content: "test", status: "pending" },
              { content: "drop", status: "abandoned" },
              { content: "wait", status: "blocked" },
            ],
          },
        ],
      };
    });

    const result = await client.call("todo.list", { sessionId: "s-1" });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.phases).toHaveLength(1);
      expect(result.value.phases[0].tasks.map((t) => t.status)).toEqual([
        "completed",
        "in_progress",
        "pending",
        "abandoned",
        "blocked",
      ]);
    }
    expect(seen).toEqual([
      { method: "_omp/desktop/v1/todo.list", params: { sessionId: "s-1" } },
    ]);
  });

  it("subagents.status round-trips", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    client.setTransport(async () => ({ enabled: true, activeCount: 2 }));
    const result = await client.call("subagents.status", {});
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value).toEqual({ enabled: true, activeCount: 2 });
    }
  });

  it("subagents.setEnabled forwards params and echoes result", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    const seen: unknown[] = [];
    client.setTransport(async (_method, params) => {
      seen.push(params);
      return { enabled: false };
    });
    const result = await client.call("subagents.setEnabled", { enabled: false });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.value).toEqual({ enabled: false });
    expect(seen).toEqual([{ enabled: false }]);
  });

  it("transport rejection maps to runtime_unavailable", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    client.setTransport(async () => {
      throw new Error("boom");
    });
    const result = await client.call("todo.list", {});
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.code).toBe("runtime_unavailable");
  });

  it("capability without transport stays fail-closed", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    const result = await client.call("todo.list", {});
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.code).toBe("runtime_unavailable");
  });
});
