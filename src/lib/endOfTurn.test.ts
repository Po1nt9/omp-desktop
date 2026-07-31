import { describe, expect, it } from "vitest";
import {
  isEndOfTurnMarker,
  mapEndOfTurnReason,
  parseEndOfTurnContent,
} from "./endOfTurn";

describe("endOfTurn", () => {
  it("maps user stop / stall / error", () => {
    expect(mapEndOfTurnReason("user_stop").reason).toBe("user_stop");
    expect(mapEndOfTurnReason("stall").messageKey).toBe("endOfTurn.stall");
    expect(mapEndOfTurnReason("permission_denied").tone).toBe("error");
    expect(mapEndOfTurnReason("error").reason).toBe("error");
  });

  it("recognizes markers", () => {
    expect(isEndOfTurnMarker("turn_cancelled")).toBe(true);
    expect(isEndOfTurnMarker("turn_end")).toBe(true);
    expect(isEndOfTurnMarker("tool_step")).toBe(false);
  });

  it("parses content", () => {
    expect(parseEndOfTurnContent("turn_end|user_stop")).toBe("user_stop");
    expect(parseEndOfTurnContent("turn_cancelled")).toBe("cancelled");
  });
});

describe("turn_interrupted / crash_recovery", () => {
  it("maps crash_recovery to interrupted chip model", () => {
    const m = mapEndOfTurnReason("crash_recovery");
    expect(m.reason).toBe("interrupted");
    expect(m.messageKey).toBe("endOfTurn.interrupted");
    expect(m.tone).toBe("error");
  });

  it("parses turn_interrupted|crash_recovery content", () => {
    expect(parseEndOfTurnContent("turn_interrupted|crash_recovery")).toBe(
      "interrupted",
    );
  });

  it("parses bare turn_interrupted content", () => {
    expect(parseEndOfTurnContent("turn_interrupted")).toBe("interrupted");
  });

  it("treats turn_interrupted as an end-of-turn marker", () => {
    expect(isEndOfTurnMarker("turn_interrupted")).toBe(true);
  });
});
