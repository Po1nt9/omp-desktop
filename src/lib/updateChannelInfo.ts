/**
 * Normalized view of the Rust `updater_status` DTO (Settings → About).
 * `channel` is the delivery mode (signed plugin vs GitHub manual);
 * `releaseChannel` is the build-baked feed identity (AC-10.9).
 */
export type ReleaseChannel = "stable" | "beta" | "nightly" | "unknown";

export type UpdaterChannelInfo = {
  /** `silent` when signed release plugin path is live; else `github_manual`. */
  channel: "silent" | "github_manual" | "unknown";
  releaseChannel: ReleaseChannel;
  pluginEnabled: boolean;
  platformSupported: boolean;
  endpoint: string;
};

export function normalizeUpdaterStatus(raw: {
  channel?: string;
  releaseChannel?: string;
  pluginEnabled?: boolean;
  platformSupported?: boolean;
  endpoint?: string;
}): UpdaterChannelInfo {
  const channel =
    raw.channel === "silent"
      ? "silent"
      : raw.channel === "github_manual"
        ? "github_manual"
        : "unknown";
  const releaseChannel: ReleaseChannel =
    raw.releaseChannel === "stable" ||
    raw.releaseChannel === "beta" ||
    raw.releaseChannel === "nightly"
      ? raw.releaseChannel
      : "unknown";
  return {
    channel,
    releaseChannel,
    pluginEnabled: !!raw.pluginEnabled,
    platformSupported: !!raw.platformSupported,
    endpoint: raw.endpoint || "",
  };
}
