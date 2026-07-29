import { describe, expect, it } from "vitest";
import { availableModels, effortDisplayLabel } from "./modelOptions";

describe("model options before runtime integration", () => {
  it("does not invent a fallback model", () => expect(availableModels).toEqual([]));
  it("keeps neutral effort labels", () => expect(effortDisplayLabel("high")).toBe("High"));
});
