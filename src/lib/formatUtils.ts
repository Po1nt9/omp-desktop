/** Pure formatting helpers with no account/billing/DTO coupling. */

export function formatCompactNumber(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "—";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  if (Number.isInteger(n)) return String(n);
  return n.toFixed(1);
}

/** Local calendar day `YYYY-MM-DD` for an ISO timestamp (heatmap / call-log filter). */
export function localDateKeyFromIso(
  iso: string | null | undefined,
): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  const d = new Date(t);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function formatDuration(secs: number | null | undefined): string {
  if (secs == null || secs <= 0) return "—";
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (m < 60) return s ? `${m}m ${s}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm ? `${h}h ${rm}m` : `${h}h`;
}

/** Compact message footer time, e.g. `星期二15:23` / `Tue 15:23`. */
export function formatMessageTime(
  iso: string | null | undefined,
  locale: string,
): string {
  if (!iso) return "";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  const d = new Date(t);
  const loc = locale === "zh" ? "zh-CN" : "en-US";
  const weekday = new Intl.DateTimeFormat(loc, { weekday: "short" }).format(d);
  const hm = new Intl.DateTimeFormat(loc, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(d);
  // zh often wants no space: 星期二15:23
  return locale === "zh" ? `${weekday}${hm}` : `${weekday} ${hm}`;
}

export function formatRelativeTime(
  iso: string | null | undefined,
  locale: string,
): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  const diff = Date.now() - t;
  const rtf = new Intl.RelativeTimeFormat(locale === "zh" ? "zh-CN" : "en", {
    numeric: "auto",
  });
  const sec = Math.round(diff / 1000);
  if (Math.abs(sec) < 60) return rtf.format(-sec, "second");
  const min = Math.round(sec / 60);
  if (Math.abs(min) < 60) return rtf.format(-min, "minute");
  const hr = Math.round(min / 60);
  if (Math.abs(hr) < 48) return rtf.format(-hr, "hour");
  const day = Math.round(hr / 24);
  return rtf.format(-day, "day");
}

/** Refresh time stamp for menus: `MM-DD HH:mm` (local). */
export function formatQuotaResetTime(
  iso: string | null | undefined,
): string {
  if (!iso) return "";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  const d = new Date(t);
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const min = String(d.getMinutes()).padStart(2, "0");
  return `${mm}-${dd} ${hh}:${min}`;
}
