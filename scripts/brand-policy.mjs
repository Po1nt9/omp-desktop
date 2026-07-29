export const textExtensions = new Set([".ts", ".tsx", ".js", ".mjs", ".rs", ".json", ".toml", ".xml", ".plist", ".html", ".md", ".yml", ".yaml", ".sh", ".py"]);

export const userVisiblePathPatterns = [
  /^src\/i18n\//,
  /^src\/components\/.*\.(?:ts|tsx)$/,
  /^src-tauri\/src\/(?:tray|tray_i18n|remote_im|mirror)\//,
  /^README(?:_EN|_ZH)?\.md$/,
];

export const rules = [
  ["grok-product-brand", /\bGrok (?:App|Desktop|Build|CLI)\b/gi],
  ["supergrok-brand", /\bSuperGrok(?:\s*Pro|\s*Heavy)?\b/gi],
  ["legacy-identifier", /\b(?:grokapp|grok_app_lib|grok_agent_stdio)\b|com\.grokapp\.desktop/gi],
  ["private-xai-method", /_x\.ai\//g],
  ["legacy-runtime-env", /\bGROK_(?:HOME|BIN|CLI|APP_ACP|APP_HOME|REMOTE_BRIDGE_HOME|CLI_ALLOW_UNVERIFIED)\b/g],
  ["legacy-runtime-path", /(?:~|\$HOME|%USERPROFILE%)?[\\/]\.grok(?:[\\/]|$)/gi],
  ["desktop-direct-xai", /https:\/\/(?:auth|accounts|api)\.x\.ai|https:\/\/(?:cli-chat-proxy|code)\.grok\.com/gi],
];

export const wholeFileAllowlist = new Set([
  "scripts/brand-policy.mjs",
  "scripts/check-provenance.mjs",
  "scripts/check-provenance.test.mjs",
  "provenance/README.md",
  "provenance/upstreams.json",
  "provenance/omp-patches.json",
  "THIRD_PARTY_NOTICES",
  "CHANGELOG.md",
  "docs/superpowers/plans/2026-07-28-repository-brand-baseline.md",
  "docs/superpowers/specs/2026-07-28-omp-desktop-design.md",
]);

export const directoryExclusions = [
  /^docs\/upstream-history\/grok-app\//,
];

export const repositoryExclusions = new Set([
  "scripts/check-brand-policy.test.mjs",
  "testdata/brand-policy/denied/app-title-grok.json",
  "testdata/brand-policy/denied/direct-auth-xai.json",
  "testdata/brand-policy/denied/legacy-identifiers.json",
  "testdata/brand-policy/denied/legacy-runtime-env.json",
  "testdata/brand-policy/denied/legacy-runtime-path.json",
  "testdata/brand-policy/denied/lowercase-brand.json",
  "testdata/brand-policy/denied/private-method-xai.json",
  "testdata/brand-policy/denied/supergrok-brand.json",
]);

export const structuredAllowlist = new Map([
  ["testdata/brand-policy/allowed/provider-xai.json", [
    { pointer: ["provider", "id"], value: "xai", rules: new Set() },
    { pointer: ["provider", "name"], value: "xAI", rules: new Set() },
    { pointer: ["provider", "endpoint"], value: "https://api.x.ai", rules: new Set(["desktop-direct-xai"]) },
    { pointer: ["provider", "authMethods", 0], value: "xAI OAuth", rules: new Set() },
  ]],
  ["testdata/brand-policy/allowed/model-grok.json", [
    { pointer: ["models", 0, "id"], value: "grok-4.5", rules: new Set() },
    { pointer: ["models", 0, "displayName"], value: "Grok 4.5", rules: new Set() },
    { pointer: ["models", 0, "providerId"], value: "xai", rules: new Set() },
  ]],
]);
