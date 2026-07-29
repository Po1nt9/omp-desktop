/**
 * Simplified first-run gate: welcome → enter home.
 *
 * The CLI install and account login steps were removed in Task 9 (Grok
 * product surface removal). The wizard now shows a welcome message and a
 * single "Get Started" button that completes setup.
 */

import { useCallback } from "react";
import { OmpLogo } from "@/components/OmpLogo";
import { runtimeAvailability } from "@/lib/runtimeAvailability";
import type { createT } from "@/i18n";

type Tr = ReturnType<typeof createT>;

export type SetupCliInfo = {
  found: boolean;
  path: string | null;
  version: string | null;
  source: string;
  cliAuthPresent: boolean;
};

type Props = {
  tr: Tr;
  platform: "mac" | "win" | "other";
  useCustomWindowChrome: boolean;
  initialCli: SetupCliInfo;
  onComplete: (cli: SetupCliInfo) => void;
  onAccountLoginOauth: () => Promise<boolean>;
};

export function SetupWizard({
  tr,
  platform,
  useCustomWindowChrome,
  initialCli,
  onComplete,
  // Kept in the Props signature for call-site compatibility; the account
  // login flow was removed in Task 9.
  onAccountLoginOauth: _onAccountLoginOauth,
}: Props) {
  const finish = useCallback(() => {
    onComplete(initialCli);
  }, [initialCli, onComplete]);

  return (
    <div
      className={
        "setup-gate" +
        (useCustomWindowChrome ? " setup-gate--custom-chrome" : "")
      }
      data-platform={platform}
      data-testid="setup-wizard"
    >
      <div className="setup-gate__drag" data-tauri-drag-region />

      <div className="setup-gate__center">
        <div className="setup-hero">
          <div className="setup-logo setup-logo--pulse">
            <OmpLogo size={44} />
          </div>
          <h1 className="setup-title">{tr("setup.title")}</h1>
          <p className="setup-subtitle">{tr("setup.subtitle")}</p>
        </div>

        <div className="setup-card">
          <div className="setup-card__head">
            <h2>{tr("setup.cli.required")}</h2>
            <p>
              {runtimeAvailability.available
                ? tr("setup.cli.foundHint", { version: "—" })
                : "OMP Runtime is not connected. Agent execution will be disabled until a runtime integration is available."}
            </p>
          </div>

          <div className="setup-actions">
            <button
              type="button"
              className="btn btn--primary setup-btn-primary"
              onClick={finish}
            >
              {tr("setup.continue")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
