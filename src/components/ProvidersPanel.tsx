/**
 * Settings → Account → Custom providers.
 * Left list + right detail/form.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type MouseEvent,
} from "react";
import * as api from "@/lib/api";
import { createT, type Locale, type MessageKey } from "@/i18n";
import { Select } from "@/components/Select";
import { GlassModal } from "@/components/GlassModal";
import {
  IconCheck,
  IconClose,
  IconEdit,
  IconPlus,
  IconRefresh,
  IconTrash,
} from "@/components/icons";

export interface ProvidersPanelProps {
  locale: Locale;
  /** Official OAuth / CLI auth / official API key present. */
  officialAvailable?: boolean;
  /** Called after switching official/custom so host can reconnect OMP Runtime. */
  onProviderActivated?: () => void;
}

type FormState = {
  id: string;
  name: string;
  baseUrl: string;
  model: string;
  apiKey: string;
  apiBackend: string;
  setAsDefault: boolean;
};

type RightMode = "empty" | "create" | "edit" | "official";
type Selection = null | "official" | string;

const emptyForm = (): FormState => ({
  id: "",
  name: "",
  baseUrl: "",
  model: "",
  apiKey: "",
  apiBackend: "responses",
  setAsDefault: true,
});

function slugify(raw: string): string {
  return raw
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
}

function hostOf(url: string): string {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}

function ccSwitchStatusKey(status: string): MessageKey {
  switch (status) {
    case "importable":
      return "prov.ccSwitch.status.importable";
    case "official":
      return "prov.ccSwitch.status.official";
    case "missing_key":
      return "prov.ccSwitch.status.missing_key";
    case "proxy_managed":
      return "prov.ccSwitch.status.proxy_managed";
    case "exists":
      return "prov.ccSwitch.status.exists";
    case "invalid":
    default:
      return "prov.ccSwitch.status.invalid";
  }
}

export function ProvidersPanel({
  locale,
  officialAvailable = false,
  onProviderActivated,
}: ProvidersPanelProps) {
  const tr = useMemo(() => createT(locale), [locale]);
  const [list, setList] = useState<api.ProvidersListResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection>(null);
  const [rightMode, setRightMode] = useState<RightMode>("empty");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [busy, setBusy] = useState(false);
  const [showKey, setShowKey] = useState(false);
  const [remoteModels, setRemoteModels] = useState<string[]>([]);
  const [hint, setHint] = useState<string | null>(null);
  const [hintTone, setHintTone] = useState<"ok" | "err" | "muted">("muted");
  const [deleteTarget, setDeleteTarget] = useState<{
    id: string;
    name: string;
  } | null>(null);
  /** Official xAI API key (for speech / STT when not using OAuth). */
  const [hasOfficialKey, setHasOfficialKey] = useState(false);
  const [officialKeyDraft, setOfficialKeyDraft] = useState("");
  const [showOfficialKey, setShowOfficialKey] = useState(false);
  const [officialKeyBusy, setOfficialKeyBusy] = useState(false);

  /** CC Switch import dialog */
  const [ccImportOpen, setCcImportOpen] = useState(false);
  const [ccScan, setCcScan] = useState<api.CcSwitchScanResult | null>(null);
  const [ccScanBusy, setCcScanBusy] = useState(false);
  const [ccImportBusy, setCcImportBusy] = useState(false);
  const [ccSelected, setCcSelected] = useState<Set<string>>(new Set());
  const [ccImportMsg, setCcImportMsg] = useState<string | null>(null);

  const protocolOptions = useMemo(
    () => [
      { value: "responses", label: tr("prov.protocol.responses") },
      {
        value: "chat_completions",
        label: tr("prov.protocol.chatCompletions"),
      },
      { value: "messages", label: tr("prov.protocol.messages") },
    ],
    [tr],
  );

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      if (!api.isTauri()) {
        setList({
          providers: [],
          defaultModel: null,
          activeSource: "official",
          activeProviderId: null,
          configPath: "",
          agentHome: "",
        });
        setHasOfficialKey(false);
        return;
      }
      const [r, masked] = await Promise.all([
        api.providersList(),
        api.secretsGetMasked().catch(() => null),
      ]);
      setList(r);
      setHasOfficialKey(!!masked?.hasOfficialKey);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const providers = list?.providers ?? [];
  const activeSource = list?.activeSource ?? "official";
  const activeProviderId = list?.activeProviderId ?? null;
  const officialActive = activeSource === "official";
  /** Show official row even without OAuth so users can paste an API key for speech. */
  const showOfficialRow = true;

  const openCreate = () => {
    setSelection(null);
    setEditingId(null);
    setForm(emptyForm());
    setRemoteModels([]);
    setHint(null);
    setShowKey(false);
    setRightMode("create");
  };

  const openCcImport = () => {
    setCcImportOpen(true);
    setCcImportMsg(null);
    setCcScan(null);
    setCcSelected(new Set());
    void runCcScan();
  };

  const runCcScan = async () => {
    if (!api.isTauri()) {
      setCcScan({
        status: "not_found",
        triedPaths: [],
        items: [],
        error: tr("prov.ccSwitch.needTauri"),
      });
      return;
    }
    setCcScanBusy(true);
    setCcImportMsg(null);
    try {
      const r = await api.providersCcSwitchScan();
      setCcScan(r);
      if (r.status === "ok") {
        const next = new Set<string>();
        for (const it of r.items) {
          // Default conflict = overwrite, so existing ids are selectable too.
          if (it.status === "importable" || it.status === "exists") {
            next.add(it.sourceId);
          }
        }
        setCcSelected(next);
      } else {
        setCcSelected(new Set());
      }
    } catch (e) {
      setCcScan({
        status: "error",
        triedPaths: [],
        items: [],
        error: String(e),
      });
    } finally {
      setCcScanBusy(false);
    }
  };

  const toggleCcItem = (sourceId: string, selectable: boolean) => {
    if (!selectable) return;
    setCcSelected((prev) => {
      const n = new Set(prev);
      if (n.has(sourceId)) n.delete(sourceId);
      else n.add(sourceId);
      return n;
    });
  };

  const runCcImport = async () => {
    if (!api.isTauri() || ccSelected.size === 0) return;
    setCcImportBusy(true);
    setCcImportMsg(null);
    try {
      // Always overwrite same id; never auto-activate route after import.
      const r = await api.providersCcSwitchImport({
        sourceIds: Array.from(ccSelected),
        onConflict: "overwrite",
        activateId: null,
      });
      if (r.providers) setList(r.providers);
      const failN = r.failed?.length ?? 0;
      if (r.imported > 0) {
        await reload();
      }
      // Success with at least one imported → close dialog (toast-style summary optional).
      if (r.imported > 0 && failN === 0) {
        setCcImportOpen(false);
        setCcImportMsg(null);
        setHint(
          tr("prov.ccSwitch.importDone", {
            n: String(r.imported),
            skipped: String(r.skipped),
            failed: String(failN),
          }),
        );
        setHintTone("ok");
      } else {
        setCcImportMsg(
          tr("prov.ccSwitch.importDone", {
            n: String(r.imported),
            skipped: String(r.skipped),
            failed: String(failN),
          }),
        );
      }
    } catch (e) {
      setCcImportMsg(String(e));
    } finally {
      setCcImportBusy(false);
    }
  };

  const openOfficial = () => {
    setSelection("official");
    setEditingId(null);
    setRightMode("official");
    setHint(null);
    setOfficialKeyDraft("");
    setShowOfficialKey(false);
  };

  const saveOfficialKey = async () => {
    const key = officialKeyDraft.trim();
    if (!key || !api.isTauri()) return;
    setOfficialKeyBusy(true);
    setHint(null);
    try {
      await api.secretsSet({ officialApiKey: key });
      setOfficialKeyDraft("");
      setShowOfficialKey(false);
      setHasOfficialKey(true);
      setHint(tr("prov.officialKeySaved"));
      setHintTone("ok");
      onProviderActivated?.();
    } catch (e) {
      setHint(String(e));
      setHintTone("err");
    } finally {
      setOfficialKeyBusy(false);
    }
  };

  const clearOfficialKey = async () => {
    if (!api.isTauri() || !hasOfficialKey) return;
    setOfficialKeyBusy(true);
    setHint(null);
    try {
      await api.secretsSet({ officialApiKey: "" });
      setHasOfficialKey(false);
      setOfficialKeyDraft("");
      setHint(tr("prov.officialKeyCleared"));
      setHintTone("muted");
      onProviderActivated?.();
    } catch (e) {
      setHint(String(e));
      setHintTone("err");
    } finally {
      setOfficialKeyBusy(false);
    }
  };

  const openEdit = (p: api.CustomProvider) => {
    setSelection(p.id);
    setEditingId(p.id);
    setForm({
      id: p.id,
      name: p.name,
      baseUrl: p.baseUrl,
      model: p.model,
      apiKey: "",
      apiBackend: p.apiBackend || "responses",
      setAsDefault: p.isDefault,
    });
    setRemoteModels([]);
    setHint(null);
    setShowKey(false);
    setRightMode("edit");
  };

  const closeRight = () => {
    setRightMode("empty");
    setSelection(null);
    setEditingId(null);
    setHint(null);
    setRemoteModels([]);
  };

  const save = async () => {
    if (!form.baseUrl.trim()) {
      setHint(tr("prov.err.needBase"));
      setHintTone("err");
      return;
    }
    if (!editingId && !form.apiKey.trim()) {
      setHint(tr("prov.err.needKey"));
      setHintTone("err");
      return;
    }
    setBusy(true);
    setHint(tr("prov.saving"));
    setHintTone("muted");
    try {
      const id =
        editingId ??
        (slugify(form.id || form.name || form.baseUrl) ||
          `provider-${Date.now().toString(36)}`);
      const r = await api.providersUpsert({
        id,
        model: form.model.trim() || id,
        baseUrl: form.baseUrl.trim(),
        name: form.name.trim() || id,
        apiKey: form.apiKey.trim() || undefined,
        apiBackend: form.apiBackend,
        setAsDefault: form.setAsDefault,
        createOnly: !editingId,
      });
      setList(r);
      const saved = r.providers.find((p) => p.id === id);
      if (saved) {
        openEdit(saved);
      } else {
        setRightMode("empty");
        setSelection(null);
      }
      setHint(null);
      if (form.setAsDefault) {
        onProviderActivated?.();
      }
    } catch (e) {
      setHint(String(e));
      setHintTone("err");
    } finally {
      setBusy(false);
    }
  };

  const confirmRemove = async () => {
    if (!deleteTarget) return;
    const { id } = deleteTarget;
    setBusy(true);
    setDeleteTarget(null);
    try {
      const r = await api.providersRemove(id);
      setList(r);
      if (editingId === id || selection === id) {
        closeRight();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const activateOfficial = async (e?: MouseEvent) => {
    e?.stopPropagation();
    setBusy(true);
    try {
      const r = await api.providersActivate("official");
      setList(r);
      onProviderActivated?.();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const activateCustom = async (id: string, e?: MouseEvent) => {
    e?.stopPropagation();
    setBusy(true);
    try {
      const r = await api.providersActivate("custom", id);
      setList(r);
      onProviderActivated?.();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const fetchModels = async () => {
    if (!form.baseUrl.trim()) {
      setHint(tr("prov.err.needBase"));
      setHintTone("err");
      return;
    }
    setHint(tr("prov.fetching"));
    setHintTone("muted");
    try {
      const r = await api.providersListModels({
        baseUrl: form.baseUrl.trim(),
        apiKey: form.apiKey.trim() || undefined,
        providerId: editingId ?? undefined,
      });
      setRemoteModels(r.models.map((m) => m.id));
      if (r.models.length) {
        setHint(tr("prov.loaded", { n: r.models.length }));
        setHintTone("ok");
        if (!form.model && r.models[0]?.id) {
          setForm((f) => ({ ...f, model: r.models[0].id }));
        }
      } else {
        setHint(tr("prov.emptyList"));
        setHintTone("muted");
      }
    } catch (e) {
      setHint(String(e));
      setHintTone("err");
    }
  };

  if (loading) {
    return (
      <div className="prov-panel" data-testid="providers-panel">
        <div className="prov-loading">{tr("prov.loading")}</div>
      </div>
    );
  }

  const listEmpty = !showOfficialRow && providers.length === 0;

  return (
    <div className="prov-panel" data-testid="providers-panel">
      {error && (
        <div className="prov-alert" role="alert">
          <span>{error}</span>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            onClick={() => setError(null)}
          >
            {tr("common.dismiss")}
          </button>
        </div>
      )}

      <div className="prov-split">
        {/* ── Left: list ───────────────────────────────────────────── */}
        <aside className="prov-split__list">
          <div className="prov-list-actions">
            <button
              type="button"
              className="btn btn--solid prov-add-btn"
              onClick={openCreate}
              disabled={busy}
            >
              <IconPlus size={16} />
              {tr("prov.new")}
            </button>
            <button
              type="button"
              className="btn btn--ghost prov-cc-import-btn"
              onClick={openCcImport}
              disabled={busy || !api.isTauri()}
              data-testid="prov-cc-switch-import"
              title={tr("prov.ccSwitch.importBtnHint")}
            >
              {tr("prov.ccSwitch.importBtn")}
            </button>
          </div>

          <div className="prov-rail" role="list">
            {showOfficialRow && (
              <div
                role="listitem"
                className={
                  "prov-item" +
                  (selection === "official" ? " is-selected" : "") +
                  (officialActive ? " is-active" : "")
                }
              >
                <button
                  type="button"
                  className="prov-item__main"
                  onClick={openOfficial}
                >
                  <span className="prov-item__avatar" aria-hidden>
                    G
                  </span>
                  <span className="prov-item__text">
                    <span className="prov-item__name">
                      {tr("prov.officialName")}
                    </span>
                    {(hasOfficialKey || officialAvailable) && (
                      <span className="prov-item__sub">
                        {officialAvailable
                          ? tr("prov.officialAuthOk")
                          : tr("prov.officialKeyOnly")}
                      </span>
                    )}
                  </span>
                </button>
                {officialAvailable ? (
                  !officialActive ? (
                    <button
                      type="button"
                      className="btn btn--ghost btn--sm prov-item__use"
                      disabled={busy}
                      onClick={(e) => void activateOfficial(e)}
                    >
                      {tr("prov.useThis")}
                    </button>
                  ) : (
                    <span
                      className="prov-item__using"
                      title={tr("prov.active")}
                      aria-label={tr("prov.active")}
                    >
                      <IconCheck size={14} />
                    </span>
                  )
                ) : null}
              </div>
            )}

            {providers.map((p) => {
              const active =
                activeSource === "custom" && activeProviderId === p.id;
              const selected = selection === p.id;
              return (
                <div
                  key={p.id}
                  role="listitem"
                  className={
                    "prov-item" +
                    (selected ? " is-selected" : "") +
                    (active ? " is-active" : "")
                  }
                >
                  <button
                    type="button"
                    className="prov-item__main"
                    onClick={() => openEdit(p)}
                  >
                    <span className="prov-item__avatar" aria-hidden>
                      {(p.name || p.id).slice(0, 1).toUpperCase()}
                    </span>
                    <span className="prov-item__text">
                      <span className="prov-item__name">{p.name || p.id}</span>
                      <span className="prov-item__sub">
                        {hostOf(p.baseUrl)}
                        {p.model ? ` · ${p.model}` : ""}
                      </span>
                    </span>
                  </button>
                  {!active ? (
                    <button
                      type="button"
                      className="btn btn--ghost btn--sm prov-item__use"
                      disabled={busy}
                      onClick={(e) => void activateCustom(p.id, e)}
                    >
                      {tr("prov.useThis")}
                    </button>
                  ) : (
                    <span
                      className="prov-item__using"
                      title={tr("prov.active")}
                      aria-label={tr("prov.active")}
                    >
                      <IconCheck size={14} />
                    </span>
                  )}
                </div>
              );
            })}

            {listEmpty && (
              <div className="prov-rail-empty">{tr("prov.emptyTitle")}</div>
            )}
          </div>
        </aside>

        {/* ── Right: detail / form ─────────────────────────────────── */}
        <section className="prov-split__detail">
          {rightMode === "empty" && (
            <div className="prov-detail-empty">
              <p>{tr("prov.detailEmpty")}</p>
            </div>
          )}

          {rightMode === "official" && (
            <div className="prov-detail settings-card">
              <div className="prov-detail__head">
                <div>
                  <h3 className="prov-detail__title">
                    {tr("prov.officialName")}
                  </h3>
                  <p className="prov-detail__sub">
                    {tr("prov.officialDesc")}
                  </p>
                </div>
                {officialAvailable ? (
                  officialActive ? (
                    <span className="account-badge account-badge--ok">
                      {tr("prov.active")}
                    </span>
                  ) : (
                    <button
                      type="button"
                      className="btn btn--solid"
                      disabled={busy}
                      onClick={() => void activateOfficial()}
                    >
                      {tr("prov.useThis")}
                    </button>
                  )
                ) : null}
              </div>
              <p className="prov-detail__sub" id="settings-anchor-official-key">
                {tr("prov.officialVoiceHint")}
              </p>
              <label className="prov-field">
                <span className="prov-field__label">
                  {tr("prov.officialApiKey")}
                </span>
                <div className="prov-key-row">
                  <input
                    className="settings-input"
                    type={showOfficialKey ? "text" : "password"}
                    value={officialKeyDraft}
                    onChange={(e) => setOfficialKeyDraft(e.target.value)}
                    placeholder={
                      hasOfficialKey
                        ? tr("prov.keyKeep")
                        : tr("prov.officialKeyPh")
                    }
                    autoComplete="off"
                    spellCheck={false}
                    disabled={officialKeyBusy}
                  />
                  <button
                    type="button"
                    className="btn btn--ghost btn--sm"
                    onClick={() => setShowOfficialKey((v) => !v)}
                  >
                    {showOfficialKey ? tr("prov.keyHide") : tr("prov.keyShow")}
                  </button>
                </div>
              </label>
              <div className="prov-form__actions">
                <button
                  type="button"
                  className="btn btn--solid"
                  disabled={
                    officialKeyBusy || !officialKeyDraft.trim() || !api.isTauri()
                  }
                  onClick={() => void saveOfficialKey()}
                >
                  {officialKeyBusy
                    ? tr("prov.saving")
                    : tr("prov.officialKeySave")}
                </button>
                {hasOfficialKey ? (
                  <button
                    type="button"
                    className="btn btn--ghost"
                    disabled={officialKeyBusy}
                    onClick={() => void clearOfficialKey()}
                  >
                    {tr("prov.officialKeyClear")}
                  </button>
                ) : null}
              </div>
              {hasOfficialKey ? (
                <p className="prov-detail__sub">{tr("prov.officialKeyPresent")}</p>
              ) : null}
              {!officialAvailable ? (
                <p className="prov-detail__sub">{tr("prov.officialLoginHint")}</p>
              ) : null}
              {hint && rightMode === "official" ? (
                <p
                  className={
                    "prov-hint" +
                    (hintTone === "ok"
                      ? " prov-hint--ok"
                      : hintTone === "err"
                        ? " prov-hint--err"
                        : "")
                  }
                  role="status"
                >
                  {hint}
                </p>
              ) : null}
            </div>
          )}

          {(rightMode === "create" || rightMode === "edit") && (
            <div
              className="prov-detail settings-card prov-form"
              data-testid="provider-form"
            >
              <div className="prov-form__head">
                <h3 className="prov-detail__title">
                  {editingId ? tr("prov.editTitle") : tr("prov.addTitle")}
                </h3>
                <button
                  type="button"
                  className="chrome-btn"
                  onClick={closeRight}
                  aria-label={tr("common.close")}
                >
                  <IconClose size={16} />
                </button>
              </div>

              <div className="prov-form__grid">
                <label className="prov-field">
                  <span className="prov-field__label">{tr("prov.name")}</span>
                  <input
                    className="settings-input"
                    value={form.name}
                    onChange={(e) => {
                      const name = e.target.value;
                      setForm((f) => ({
                        ...f,
                        name,
                        id: editingId ? f.id : slugify(name) || f.id,
                      }));
                    }}
                    placeholder={tr("prov.namePh")}
                    autoComplete="off"
                  />
                </label>

                {!editingId && (
                  <label className="prov-field">
                    <span className="prov-field__label">
                      {tr("prov.displayName")}
                    </span>
                    <input
                      className="settings-input"
                      value={form.id}
                      onChange={(e) =>
                        setForm((f) => ({
                          ...f,
                          id: slugify(e.target.value),
                        }))
                      }
                      placeholder={tr("prov.idPh")}
                      autoComplete="off"
                      spellCheck={false}
                    />
                  </label>
                )}

                <label className="prov-field prov-field--full">
                  <span className="prov-field__label">{tr("prov.baseUrl")}</span>
                  <input
                    className="settings-input"
                    value={form.baseUrl}
                    onChange={(e) =>
                      setForm((f) => ({ ...f, baseUrl: e.target.value }))
                    }
                    placeholder={tr("prov.baseUrlPh")}
                    autoComplete="off"
                    spellCheck={false}
                  />
                </label>

                <div className="prov-field">
                  <span className="prov-field__label">{tr("prov.protocol")}</span>
                  <Select
                    value={form.apiBackend}
                    onChange={(v) =>
                      setForm((f) => ({ ...f, apiBackend: v }))
                    }
                    options={protocolOptions}
                    aria-label={tr("prov.protocol")}
                  />
                </div>

                <label className="prov-field">
                  <span className="prov-field__label">{tr("prov.apiKey")}</span>
                  <div className="prov-key-row">
                    <input
                      className="settings-input"
                      type={showKey ? "text" : "password"}
                      value={form.apiKey}
                      onChange={(e) =>
                        setForm((f) => ({ ...f, apiKey: e.target.value }))
                      }
                      placeholder={
                        editingId ? tr("prov.keyKeep") : tr("prov.keyPh")
                      }
                      autoComplete="new-password"
                      spellCheck={false}
                    />
                    <button
                      type="button"
                      className="btn btn--ghost btn--sm"
                      onClick={() => setShowKey((v) => !v)}
                    >
                      {showKey ? tr("prov.keyHide") : tr("prov.keyShow")}
                    </button>
                  </div>
                </label>

                <label className="prov-field prov-field--full">
                  <span className="prov-field__label-row">
                    <span className="prov-field__label">
                      {tr("prov.requestModel")}
                    </span>
                    <button
                      type="button"
                      className="btn btn--ghost btn--sm"
                      onClick={() => void fetchModels()}
                      disabled={busy}
                    >
                      <IconRefresh size={14} />
                      {tr("prov.fetchModels")}
                    </button>
                  </span>
                  <input
                    className="settings-input"
                    value={form.model}
                    onChange={(e) =>
                      setForm((f) => ({ ...f, model: e.target.value }))
                    }
                    placeholder={tr("prov.modelPh")}
                    list="prov-model-suggestions"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <datalist id="prov-model-suggestions">
                    {remoteModels.map((m) => (
                      <option key={m} value={m} />
                    ))}
                  </datalist>
                </label>
              </div>

              <label className="prov-check">
                <input
                  type="checkbox"
                  checked={form.setAsDefault}
                  onChange={(e) =>
                    setForm((f) => ({
                      ...f,
                      setAsDefault: e.target.checked,
                    }))
                  }
                />
                <span className="prov-check__title">
                  {tr("prov.setDefault")}
                </span>
              </label>

              {hint && (
                <div
                  className={
                    "prov-form__hint" +
                    (hintTone === "ok"
                      ? " is-ok"
                      : hintTone === "err"
                        ? " is-err"
                        : "")
                  }
                >
                  {hint}
                </div>
              )}

              <div className="prov-form__actions">
                {editingId && (
                  <button
                    type="button"
                    className="btn btn--danger"
                    disabled={busy}
                    onClick={() =>
                      setDeleteTarget({
                        id: editingId,
                        name: form.name || editingId,
                      })
                    }
                  >
                    <IconTrash size={14} />
                    {tr("prov.delete")}
                  </button>
                )}
                <div className="prov-form__actions-end">
                  <button
                    type="button"
                    className="btn btn--ghost"
                    onClick={closeRight}
                    disabled={busy}
                  >
                    {tr("common.cancel")}
                  </button>
                  <button
                    type="button"
                    className="btn btn--solid"
                    onClick={() => void save()}
                    disabled={busy}
                  >
                    {editingId ? (
                      <>
                        <IconEdit size={14} />
                        {tr("prov.save")}
                      </>
                    ) : (
                      <>
                        <IconPlus size={14} />
                        {tr("prov.add")}
                      </>
                    )}
                  </button>
                </div>
              </div>
            </div>
          )}
        </section>
      </div>

      <GlassModal
        open={!!deleteTarget}
        onClose={() => setDeleteTarget(null)}
        title={tr("prov.delete")}
        size="sm"
        closeLabel={tr("common.close")}
        footer={
          <>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setDeleteTarget(null)}
            >
              {tr("common.cancel")}
            </button>
            <button
              type="button"
              className="btn btn--danger"
              onClick={() => void confirmRemove()}
            >
              {tr("prov.delete")}
            </button>
          </>
        }
      >
        <p className="prov-delete-msg">
          {tr("prov.confirmDelete", {
            id: deleteTarget?.name || deleteTarget?.id || "",
          })}
        </p>
      </GlassModal>

      <GlassModal
        open={ccImportOpen}
        onClose={() => !ccImportBusy && setCcImportOpen(false)}
        title={tr("prov.ccSwitch.title")}
        size="lg"
        closeLabel={tr("common.close")}
        wrapBody
        bodyClassName="prov-cc-modal-body"
        footer={
          <>
            <button
              type="button"
              className="btn btn--ghost"
              disabled={ccImportBusy}
              onClick={() => setCcImportOpen(false)}
            >
              {tr("common.cancel")}
            </button>
            <button
              type="button"
              className="btn btn--ghost"
              disabled={ccScanBusy || ccImportBusy}
              onClick={() => void runCcScan()}
            >
              <IconRefresh size={14} />
              {tr("prov.ccSwitch.rescan")}
            </button>
            <button
              type="button"
              className="btn btn--solid"
              disabled={
                ccImportBusy ||
                ccScanBusy ||
                ccSelected.size === 0 ||
                ccScan?.status !== "ok"
              }
              onClick={() => void runCcImport()}
            >
              {ccImportBusy
                ? tr("prov.ccSwitch.importing")
                : tr("prov.ccSwitch.importAction", {
                    n: String(ccSelected.size),
                  })}
            </button>
          </>
        }
      >
        {ccScanBusy && !ccScan ? (
          <p className="prov-cc-status" role="status">
            {tr("prov.ccSwitch.scanning")}
          </p>
        ) : null}

        {ccScan?.status === "not_found" ? (
          <div className="prov-cc-empty">
            <p>{tr("prov.ccSwitch.notFound")}</p>
            <p className="prov-cc-muted">{tr("prov.ccSwitch.notFoundHint")}</p>
            {ccScan.triedPaths.length > 0 ? (
              <details className="prov-cc-paths">
                <summary>{tr("prov.ccSwitch.triedPaths")}</summary>
                <ul>
                  {ccScan.triedPaths.map((p) => (
                    <li key={p}>
                      <code>{p}</code>
                    </li>
                  ))}
                </ul>
              </details>
            ) : null}
          </div>
        ) : null}

        {ccScan?.status === "error" ? (
          <div className="prov-cc-empty" role="alert">
            <p>{tr("prov.ccSwitch.scanError")}</p>
            <p className="prov-cc-muted">{ccScan.error}</p>
          </div>
        ) : null}

        {ccScan?.status === "ok" ? (
          <>
            <p className="prov-cc-muted">
              {tr("prov.ccSwitch.found", {
                n: String(ccScan.items.length),
                path: ccScan.dbPath || "",
              })}
            </p>
            {ccScan.items.length === 0 ? (
              <p className="prov-cc-empty">{tr("prov.ccSwitch.noItems")}</p>
            ) : (
              <ul className="prov-cc-list" role="list">
                {ccScan.items.map((it) => {
                  const selectable =
                    it.status === "importable" || it.status === "exists";
                  const checked = ccSelected.has(it.sourceId);
                  return (
                    <li
                      key={it.sourceId}
                      className={
                        "prov-cc-item" +
                        (checked ? " is-checked" : "") +
                        (!selectable ? " is-disabled" : "")
                      }
                    >
                      <label className="prov-cc-item__row">
                        <input
                          type="checkbox"
                          checked={checked}
                          disabled={!selectable || ccImportBusy}
                          onChange={() =>
                            toggleCcItem(it.sourceId, selectable)
                          }
                        />
                        <span className="prov-cc-item__main">
                          <span className="prov-cc-item__name">
                            {it.name}
                            {it.isCurrent ? (
                              <span className="prov-cc-badge">
                                {tr("prov.ccSwitch.current")}
                              </span>
                            ) : null}
                          </span>
                          <span className="prov-cc-item__sub">
                            {it.baseUrl || "—"}
                            {it.model ? ` · ${it.model}` : ""}
                            {it.apiBackend ? ` · ${it.apiBackend}` : ""}
                          </span>
                          <span
                            className={
                              "prov-cc-item__status prov-cc-item__status--" +
                              it.status
                            }
                          >
                            {tr(ccSwitchStatusKey(it.status))}
                            {it.statusDetail
                              ? ` — ${it.statusDetail}`
                              : it.keyHint
                                ? ` · ${it.keyHint}`
                                : ""}
                          </span>
                        </span>
                      </label>
                    </li>
                  );
                })}
              </ul>
            )}
          </>
        ) : null}

        {ccImportMsg ? (
          <p className="prov-cc-result" role="status">
            {ccImportMsg}
          </p>
        ) : null}
      </GlassModal>
    </div>
  );
}
