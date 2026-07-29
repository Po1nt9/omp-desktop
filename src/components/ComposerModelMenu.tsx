/**
 * Composer chip menus (Codex-style):
 * - Model (+effort)
 * - Access: session mode + permission in one panel
 * Narrow composer widths compress triggers to icon (+ short label).
 */

import {
  useEffect,
  useId,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import {
  availableModels,
  PERMISSION_POLICIES,
  SESSION_MODES,
  effortDisplayLabel,
  effortsForModel,
  findModel,
  type ModelOption,
  type PermissionPolicyId,
} from "@/lib/modelOptions";
import { Tip } from "@/components/ui/tooltip";
import {
  IconAlertTriangle,
  IconBolt,
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconHandStop,
  IconList,
  IconRobot,
  IconShield,
  IconShieldCheck,
} from "@/components/icons";
import { useFloatingMenu, type FloatingPos } from "@/lib/floatingMenu";

type Nested = "model" | "effort" | null;

function usePortalMenu(estHeight = 220, _width = 300, nestedKey?: string) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popRef = useRef<HTMLDivElement>(null);
  const popId = useId();

  const { pos, style: popStyle } = useFloatingMenu({
    open,
    triggerRef,
    panelRef: popRef,
    roots: [rootRef],
    onClose: () => setOpen(false),
    placement: "auto",
    fitContent: true,
    minWidth: 200,
    estHeight,
    gap: 8,
    deps: [nestedKey],
  });

  return {
    open,
    setOpen,
    pos,
    popStyle: popStyle as CSSProperties | undefined,
    rootRef,
    triggerRef,
    popRef,
    popId,
  };
}

function MenuShell({
  open,
  setOpen,
  rootRef,
  triggerRef,
  popRef,
  popId,
  pos,
  popStyle,
  triggerIcon,
  triggerText,
  triggerShort,
  ariaLabel,
  title,
  danger,
  children,
  onOpenChange,
  className = "",
}: {
  open: boolean;
  setOpen: (v: boolean | ((p: boolean) => boolean)) => void;
  rootRef: React.RefObject<HTMLDivElement | null>;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  popRef: React.RefObject<HTMLDivElement | null>;
  popId: string;
  pos: FloatingPos | null;
  popStyle: CSSProperties | undefined;
  triggerIcon?: ReactNode;
  /** Full label (wide layout) */
  triggerText: string;
  /** Short label (medium; icon-only when very narrow via CSS) */
  triggerShort?: string;
  ariaLabel: string;
  title?: string;
  danger?: boolean;
  children: ReactNode;
  onOpenChange?: (open: boolean) => void;
  className?: string;
}) {
  const panel =
    open && pos && typeof document !== "undefined"
      ? createPortal(
          <div
            ref={popRef}
            className="cmm__pop cmm__pop--portal"
            id={popId}
            role="dialog"
            aria-label={ariaLabel}
            style={popStyle}
          >
            {children}
          </div>,
          document.body,
        )
      : null;

  const tipLabel = title ?? ariaLabel;
  const trigger = (
    <button
      ref={triggerRef}
      type="button"
      className="cmm__trigger"
      aria-haspopup="dialog"
      aria-expanded={open}
      aria-controls={popId}
      aria-label={ariaLabel}
      onClick={() => {
        setOpen((v) => {
          const next = !v;
          onOpenChange?.(next);
          return next;
        });
      }}
    >
      {triggerIcon ? (
        <span className="cmm__icon" aria-hidden>
          {triggerIcon}
        </span>
      ) : null}
      <span className="cmm__trigger-text cmm__trigger-text--full">
        {triggerText}
      </span>
      {triggerShort != null && (
        <span className="cmm__trigger-text cmm__trigger-text--short">
          {triggerShort}
        </span>
      )}
      <span className="cmm__chev" aria-hidden>
        <IconChevronDown size={12} />
      </span>
    </button>
  );

  return (
    <div
      ref={rootRef}
      className={`cmm ${open ? "is-open" : ""} ${danger ? "cmm--danger" : ""} ${className}`.trim()}
    >
      {tipLabel ? <Tip label={tipLabel}>{trigger}</Tip> : trigger}
      {panel}
    </div>
  );
}

/* ---------- Model + effort ---------- */

export interface ComposerModelMenuProps {
  modelId: string;
  effort: string;
  /** Live selectable models only (from Host catalog). */
  models?: readonly ModelOption[];
  labels: {
    model: string;
    effort: string;
    effortHigh: string;
    effortMedium: string;
    effortLow: string;
  };
  onModel: (id: string) => void;
  onEffort: (id: string) => void;
}

function resolveEffortLabel(
  effortId: string,
  effortList: ReturnType<typeof effortsForModel>,
  labels: ComposerModelMenuProps["labels"],
): string {
  const entry = effortList.find((e) => e.id === effortId);
  return effortDisplayLabel(entry ?? effortId, {
    high: labels.effortHigh,
    medium: labels.effortMedium,
    low: labels.effortLow,
  });
}

export function ComposerModelMenu({
  modelId,
  effort,
  models = availableModels,
  labels,
  onModel,
  onEffort,
}: ComposerModelMenuProps) {
  const [nested, setNested] = useState<Nested>(null);
  const menu = usePortalMenu(240, 280, nested ?? "root");
  const modelList = models.length > 0 ? models : availableModels;
  const activeModel = findModel(modelId, modelList);
  const effortList = effortsForModel(activeModel);

  useEffect(() => {
    if (!menu.open) setNested(null);
  }, [menu.open]);

  useEffect(() => {
    if (!menu.open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && nested) {
        e.stopPropagation();
        setNested(null);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [menu.open, nested]);

  const modelLabel = activeModel?.label ?? modelId;
  const eLabel = resolveEffortLabel(effort, effortList, labels);
  // Compact trigger: model + short effort (locale), no middle-dot noise.
  const triggerText = `${modelLabel} ${eLabel}`;
  const title = `${labels.model}: ${modelLabel} · ${labels.effort}: ${eLabel}`;

  return (
    <MenuShell
      {...menu}
      className="cmm--model"
      triggerIcon={<IconBolt size={14} />}
      triggerText={triggerText}
      triggerShort={eLabel}
      ariaLabel={labels.model}
      title={title}
      onOpenChange={(o) => {
        if (!o) setNested(null);
      }}
    >
      {nested === null ? (
        <>
          <button
            type="button"
            className="cmm__row"
            onClick={() => setNested("model")}
          >
            <span>{labels.model}</span>
            <span className="cmm__row-val">
              {modelLabel}
              <IconChevronRight size={14} />
            </span>
          </button>
          <button
            type="button"
            className="cmm__row"
            onClick={() => setNested("effort")}
          >
            <span>{labels.effort}</span>
            <span className="cmm__row-val">
              {eLabel}
              <IconChevronRight size={14} />
            </span>
          </button>
        </>
      ) : (
        <div className="cmm__nested">
          <button
            type="button"
            className="cmm__back"
            onClick={() => setNested(null)}
          >
            {nested === "model" ? labels.model : labels.effort}
          </button>
          {nested === "model" &&
            (modelList.length === 0 ? (
              <div className="cmm__opt cmm__opt--muted" role="status">
                <span className="cmm__opt-main">
                  <span className="cmm__opt-title">{modelId || "—"}</span>
                </span>
              </div>
            ) : (
              modelList.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  className={
                    "cmm__opt" + (m.id === modelId ? " is-active" : "")
                  }
                  onClick={() => {
                    onModel(m.id);
                    setNested(null);
                  }}
                >
                  <span className="cmm__opt-main">
                    <span className="cmm__opt-title">{m.label}</span>
                  </span>
                  {m.id === modelId && (
                    <span className="cmm__opt-check" aria-hidden>
                      <IconCheck size={16} />
                    </span>
                  )}
                </button>
              ))
            ))}
          {nested === "effort" &&
            effortList.map((e) => (
              <button
                key={e.id}
                type="button"
                className={"cmm__opt" + (e.id === effort ? " is-active" : "")}
                onClick={() => {
                  onEffort(e.id);
                  setNested(null);
                }}
              >
                <span className="cmm__opt-main">
                  <span className="cmm__opt-title">
                    {resolveEffortLabel(e.id, effortList, labels)}
                  </span>
                </span>
                {e.id === effort && (
                  <span className="cmm__opt-check" aria-hidden>
                    <IconCheck size={16} />
                  </span>
                )}
              </button>
            ))}
        </div>
      )}
    </MenuShell>
  );
}

/* ---------- Access: mode + permission (Codex-style one entry) ---------- */

export interface ComposerAccessMenuProps {
  mode: string;
  policy: string;
  labels: {
    access: string;
    accessHint: string;
    mode: string;
    modeAgent: string;
    modePlan: string;
    modeAsk: string;
    modeAgentDesc: string;
    modePlanDesc: string;
    modeAskDesc: string;
    permission: string;
    policyAsk: string;
    policyAcceptEdits: string;
    policySession: string;
    policyDontAsk: string;
    policyYolo: string;
    policyAskDesc: string;
    policyAcceptEditsDesc: string;
    policySessionDesc: string;
    policyDontAskDesc: string;
    policyYoloDesc: string;
    policyShortAsk: string;
    policyShortAccept: string;
    policyShortSession: string;
    policyShortDontAsk: string;
    policyShortYolo: string;
  };
  onMode: (id: string) => void;
  onPolicy: (id: PermissionPolicyId) => void;
}

function modeLabel(id: string, labels: ComposerAccessMenuProps["labels"]): string {
  if (id === "plan") return labels.modePlan;
  if (id === "ask") return labels.modeAsk;
  return labels.modeAgent;
}

function modeDesc(id: string, labels: ComposerAccessMenuProps["labels"]): string {
  if (id === "plan") return labels.modePlanDesc;
  if (id === "ask") return labels.modeAskDesc;
  return labels.modeAgentDesc;
}

function policyLabel(
  id: string,
  labels: ComposerAccessMenuProps["labels"],
): string {
  switch (id) {
    case "accept_edits":
      return labels.policyAcceptEdits;
    case "allow_for_session":
      return labels.policySession;
    case "dont_ask":
      return labels.policyDontAsk;
    case "always_approve":
      return labels.policyYolo;
    default:
      return labels.policyAsk;
  }
}

function policyShort(
  id: string,
  labels: ComposerAccessMenuProps["labels"],
): string {
  switch (id) {
    case "accept_edits":
      return labels.policyShortAccept;
    case "allow_for_session":
      return labels.policyShortSession;
    case "dont_ask":
      return labels.policyShortDontAsk;
    case "always_approve":
      return labels.policyShortYolo;
    default:
      return labels.policyShortAsk;
  }
}

function policyDesc(
  id: string,
  labels: ComposerAccessMenuProps["labels"],
): string {
  switch (id) {
    case "accept_edits":
      return labels.policyAcceptEditsDesc;
    case "allow_for_session":
      return labels.policySessionDesc;
    case "dont_ask":
      return labels.policyDontAskDesc;
    case "always_approve":
      return labels.policyYoloDesc;
    default:
      return labels.policyAskDesc;
  }
}

function policyIcon(id: string) {
  switch (id) {
    case "accept_edits":
      return <IconShieldCheck size={18} />;
    case "allow_for_session":
      return <IconShield size={18} />;
    case "dont_ask":
      return <IconHandStop size={18} />;
    case "always_approve":
      return <IconAlertTriangle size={18} />;
    default:
      return <IconHandStop size={18} />;
  }
}

function modeIcon(id: string) {
  if (id === "plan") return <IconList size={18} />;
  if (id === "ask") return <IconHandStop size={18} />;
  return <IconRobot size={18} />;
}

export function ComposerAccessMenu({
  mode,
  policy,
  labels,
  onMode,
  onPolicy,
}: ComposerAccessMenuProps) {
  const menu = usePortalMenu(420, 320);
  const isDanger = policy === "always_approve";
  const full = policyLabel(policy, labels);
  const short = policyShort(policy, labels);
  const title = `${labels.mode}: ${modeLabel(mode, labels)} · ${labels.permission}: ${full}`;

  return (
    <MenuShell
      {...menu}
      className="cmm--access"
      triggerIcon={policyIcon(policy)}
      triggerText={full}
      triggerShort={short}
      ariaLabel={labels.access}
      title={title}
      danger={isDanger}
    >
      <div className="cmm__header">
        <div className="cmm__header-title">{labels.accessHint}</div>
      </div>

      <div className="cmm__section">{labels.mode}</div>
      {SESSION_MODES.map((m) => (
        <button
          key={m.id}
          type="button"
          className={"cmm__opt cmm__opt--rich" + (m.id === mode ? " is-active" : "")}
          onClick={() => onMode(m.id)}
        >
          <span className="cmm__opt-icon" aria-hidden>
            {modeIcon(m.id)}
          </span>
          <span className="cmm__opt-main">
            <span className="cmm__opt-title">{modeLabel(m.id, labels)}</span>
            <span className="cmm__opt-desc">{modeDesc(m.id, labels)}</span>
          </span>
          {m.id === mode && (
            <span className="cmm__opt-check" aria-hidden>
              <IconCheck size={16} />
            </span>
          )}
        </button>
      ))}

      <div className="cmm__section cmm__section--gap">{labels.permission}</div>
      {PERMISSION_POLICIES.map((p) => (
        <button
          key={p.id}
          type="button"
          className={
            "cmm__opt cmm__opt--rich" +
            (p.id === policy ? " is-active" : "") +
            (p.dangerous ? " is-danger" : "")
          }
          onClick={() => {
            onPolicy(p.id);
            menu.setOpen(false);
          }}
        >
          <span className="cmm__opt-icon" aria-hidden>
            {policyIcon(p.id)}
          </span>
          <span className="cmm__opt-main">
            <span className="cmm__opt-title">{policyLabel(p.id, labels)}</span>
            <span className="cmm__opt-desc">{policyDesc(p.id, labels)}</span>
          </span>
          {p.id === policy && (
            <span className="cmm__opt-check" aria-hidden>
              <IconCheck size={16} />
            </span>
          )}
        </button>
      ))}
    </MenuShell>
  );
}

