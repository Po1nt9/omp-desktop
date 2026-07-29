/**
 * Fail-closed interaction test (Plan 1, Task 9, Step 1).
 *
 * Verifies the OMP Desktop shell renders a fail-closed workspace when the
 * OMP Runtime integration is absent: a visible "OMP Runtime is not connected"
 * notice is shown and the agent-execution entry point (the SetupWizard
 * "Get Started" path) does not pretend a runtime is wired up.
 *
 * The repository has no `@testing-library/react` or `jsdom` dependency and the
 * existing SSR smoke tests (see `src/components/remoteIm/ssr-smoke.test.tsx`)
 * use `renderToString` in the node environment. The App component's main
 * workspace (with the Send button) is gated by `appGate === "ready"`, which is
 * only reached inside a `useEffect` — so it is not reachable via SSR. The
 * runtime-unavailable notice is rendered by `SetupWizard`, a pure component
 * that reads the `runtimeAvailability` constant directly. We therefore verify
 * the fail-closed surface on `SetupWizard` (the surface the user sees when
 * the runtime is unavailable) and additionally assert that the App boot
 * screen renders without crashing under SSR.
 */
import { describe, it, expect, beforeAll, vi } from "vitest";
import { renderToString } from "react-dom/server";
import React from "react";
import { createT } from "@/i18n";
import {
  SetupWizard,
  type SetupCliInfo,
} from "@/components/SetupWizard";

// plyr accesses `document` at module load; mock it so SSR doesn't crash.
vi.mock("plyr", () => ({ default: { setup: () => {} } }));
// FileMediaPlayer imports plyr and CSS; mock the whole component.
vi.mock("@/components/FileMediaPlayer", () => ({
  default: () => null,
  FileMediaPlayer: () => null,
}));
// OfficeDocumentPreview imports react-pdf/xlsx/docx-preview which need DOM globals.
vi.mock("@/components/OfficeDocumentPreview", () => ({
  default: () => null,
  OfficeDocumentPreview: () => null,
}));

// Minimal browser stubs so App's useState initializers (theme/skin/layout
// loaders) do not crash under node SSR. Only properties touched during the
// initial render are stubbed.
beforeAll(() => {
  const store = new Map<string, string>();
  const ls: Storage = {
    get length() {
      return store.size;
    },
    key: (i: number) => Array.from(store.keys())[i] ?? null,
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => {
      store.set(k, String(v));
    },
    removeItem: (k: string) => {
      store.delete(k);
    },
    clear: () => {
      store.clear();
    },
  };
  (globalThis as { localStorage?: Storage }).localStorage = ls;
  (globalThis as { window?: unknown }).window ??= globalThis;
  // theme.ts reads `window.matchMedia.bind(window)` when window is defined;
  // stub it so SSR in node doesn't throw "Cannot read properties of undefined".
  const g = globalThis as { matchMedia?: unknown };
  if (typeof g.matchMedia !== "function") {
    g.matchMedia = (_query: string) => ({ matches: false, media: "", onchange: null, addListener: () => {}, removeListener: () => {}, addEventListener: () => {}, removeEventListener: () => {}, dispatchEvent: () => false });
  }
  // mirrorTransport.ts reads `window.location.pathname`; stub location so SSR
  // doesn't throw "Cannot read properties of undefined (reading 'pathname')".
  const loc = globalThis as { location?: unknown };
  if (typeof loc.location === "undefined") {
    loc.location = { pathname: "/", hostname: "localhost", href: "http://localhost/", origin: "http://localhost", protocol: "http:", port: "", search: "", hash: "" };
  }
});

describe("App fail-closed workspace", () => {
  it("surfaces the OMP Runtime unavailable notice in the setup gate", () => {
    const tr = createT("en");
    const initialCli: SetupCliInfo = {
      found: false,
      path: null,
      version: null,
      source: "",
      cliAuthPresent: false,
    };
    const html = renderToString(
      React.createElement(SetupWizard, {
        tr,
        platform: "other",
        useCustomWindowChrome: false,
        initialCli,
        onComplete: () => {},
        onAccountLoginOauth: () => Promise.resolve(false),
      }),
    );

    // Visible runtime-unavailable notice (the literal fail-closed copy).
    expect(html).toMatch(/OMP Runtime is not connected/i);
    // The runtime notice is paired with the explicit fail-closed consequence.
    expect(html).toMatch(/Agent execution will be disabled/i);
    // The wizard still renders the continue affordance so the user can
    // dismiss the gate (fail-closed ≠ hard brick).
    expect(html).toContain("setup-btn-primary");
  });

  it("renders the App boot screen without crashing under SSR", async () => {
    // App is imported lazily so its top-level module graph (which touches
    // Tauri/browser globals) is only evaluated after the mocks above are
    // registered.
    const { default: App } = await import("@/App");
    const html = renderToString(React.createElement(App));
    // The App boot screen is rendered while appGate === "loading". This must
    // not throw and must surface the setup-gate container.
    expect(html).toContain("setup-gate");
  });
});
