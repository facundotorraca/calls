import { STEP_LABELS } from "./api";

const dayFormat = new Intl.DateTimeFormat("es-AR", { day: "numeric", month: "short" });
const timeFormat = new Intl.DateTimeFormat("es-AR", { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });

/** `3 nov`, with the year only when it is not this one. */
export function formatDay(iso: string): string {
  const date = new Date(iso);
  // From the parts: some locale data writes `3 de nov`.
  const part = (type: string) => dayFormat.formatToParts(date).find((p) => p.type === type)?.value;
  const day = `${part("day")} ${part("month")}`;
  return date.getFullYear() === new Date().getFullYear() ? day : `${day} ${date.getFullYear()}`;
}

/** `3 nov · 08:21` */
export const formatDate = (iso: string) => `${formatDay(iso)} · ${timeFormat.format(new Date(iso))}`;

export function formatDuration(ms: number | null): string | null {
  if (ms == null) return null;
  const minutes = Math.round(ms / 60000);
  if (minutes < 1) return `${Math.round(ms / 1000)} s`;
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

/** `mm:ss`, or `h:mm:ss` once past the hour. Same as the Rust side. */
export function formatTimestamp(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const mm = String(Math.floor(total / 60) % 60).padStart(2, "0");
  const ss = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** `1.6 GB`, `885 KB`: decimal units, like Finder. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.round(bytes / 1e3)} KB`;
}

export const stepLabel = (step: string) => STEP_LABELS[step] ?? step;
