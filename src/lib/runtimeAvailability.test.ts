import { describe, expect, it } from "vitest";
import { runtimeAvailability } from "./runtimeAvailability";

describe("runtimeAvailability", () => {
  it("is unavailable until a later plan connects OMP", () => {
    expect(runtimeAvailability).toEqual({ available: false, reason: "runtime_unavailable" });
  });
});
