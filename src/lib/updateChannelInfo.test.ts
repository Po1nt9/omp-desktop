import { describe, expect, it } from "vitest";
import { normalizeUpdaterStatus } from "./updateChannelInfo";

describe("normalizeUpdaterStatus", () => {
  it("passes through known delivery modes and release channels", () => {
    expect(
      normalizeUpdaterStatus({
        channel: "silent",
        releaseChannel: "nightly",
        pluginEnabled: true,
        platformSupported: true,
        endpoint: "https://example/nightly.json",
      }),
    ).toEqual({
      channel: "silent",
      releaseChannel: "nightly",
      pluginEnabled: true,
      platformSupported: true,
      endpoint: "https://example/nightly.json",
    });
  });

  it("normalizes unknown / missing values defensively", () => {
    expect(normalizeUpdaterStatus({})).toEqual({
      channel: "unknown",
      releaseChannel: "unknown",
      pluginEnabled: false,
      platformSupported: false,
      endpoint: "",
    });
    expect(
      normalizeUpdaterStatus({ channel: "weird", releaseChannel: "canary" })
        .releaseChannel,
    ).toBe("unknown");
    expect(
      normalizeUpdaterStatus({ channel: "github_manual", releaseChannel: "beta" })
        .channel,
    ).toBe("github_manual");
  });

  it("accepts stable and beta release channels", () => {
    expect(normalizeUpdaterStatus({ releaseChannel: "stable" }).releaseChannel).toBe("stable");
    expect(normalizeUpdaterStatus({ releaseChannel: "beta" }).releaseChannel).toBe("beta");
  });
});
