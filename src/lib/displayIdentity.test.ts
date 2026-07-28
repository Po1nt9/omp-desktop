import { describe, expect, it } from "vitest";
import { identityInitials } from "./displayIdentity";

describe("identityInitials", () => {
  it("returns empty string for empty input", () => {
    expect(identityInitials("")).toBe("");
    expect(identityInitials("   ")).toBe("");
    expect(identityInitials("\t\n")).toBe("");
  });

  it("returns the first letter for a single word", () => {
    expect(identityInitials("Alice")).toBe("A");
    expect(identityInitials("bob")).toBe("B");
    expect(identityInitials("  carol  ")).toBe("C");
  });

  it("returns the first two letters for a two-word name", () => {
    expect(identityInitials("Alice Smith")).toBe("AS");
    expect(identityInitials("bob jones")).toBe("BJ");
    expect(identityInitials("  Carol   Brown  ")).toBe("CB");
  });

  it("caps at two initials even for long names", () => {
    expect(identityInitials("Alice Bob Carol")).toBe("AB");
    expect(identityInitials("Alice Bob Carol Dave")).toBe("AB");
  });

  it("uppercases the result", () => {
    expect(identityInitials("alice smith")).toBe("AS");
    expect(identityInitials("a b")).toBe("AB");
  });
});
