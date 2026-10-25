// Notion export: picking parent pages (by pasting their link) and a call's sync control.
// A call's page goes under its own parent page if it has one, else its folder's, else the
// default one.
import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type Call, type NotionLevel, type NotionPage, type NotionStatus, type Settings } from "./api";
import { ChevronIcon, ExternalIcon, PageIcon, SyncIcon } from "./icons";

/** The parent page: the one chosen, or a field to paste a page's link into. */
export function PagePicker({
  page,
  placeholder = "pegá el link de la página",
  onChange,
}: {
  page: NotionPage | null;
  placeholder?: string;
  onChange: (page: NotionPage | null) => Promise<void> | void;
}) {
  const [link, setLink] = useState("");
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (page) {
    return (
      <span className="page-picker">
        <PageChip page={page} />
        <button className="ghost-button tech-label" onClick={() => onChange(null)} title="Quitar">
          quitar
        </button>
      </span>
    );
  }

  const check = async () => {
    if (!link.trim() || checking) return;
    setChecking(true);
    setError(null);
    try {
      await onChange(await api.notionPage(link));
      setLink("");
    } catch (e) {
      setError(String(e));
    } finally {
      setChecking(false);
    }
  };
  return (
    <span className="page-picker column">
      <input
        className="text-input"
        value={link}
        placeholder={placeholder}
        spellCheck={false}
        disabled={checking}
        onChange={(e) => setLink(e.target.value)}
        onBlur={check}
        onKeyDown={(e) => e.key === "Enter" && check()}
      />
      {checking && <span className="field-hint">buscando la página…</span>}
      {error && <span className="model-error">{error}</span>}
    </span>
  );
}

const FROM: Record<NotionLevel, string> = {
  default: "la página por defecto",
  folder: "la página de la carpeta",
  call: "solo esta llamada",
};

/** A call's Notion control, in its header: where its page goes, sync, and open it. */
export function NotionControl({
  call,
  notion,
  onError,
}: {
  call: Call;
  notion: Settings["notion"];
  onError: (error: unknown) => void;
}) {
  const [status, setStatus] = useState<NotionStatus | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  // Again when coming back from the settings, where folder and default pages are chosen.
  const reload = useCallback(() => void api.notionStatus(call.id).then(setStatus, () => setStatus(null)), [call.id]);
  useEffect(() => {
    setStatus(null);
    setOpen(false);
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, [reload, call.folder, notion.parent?.id]);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);

  if (!notion.workspace) {
    return (
      <button className="ghost-button tech-label" onClick={() => api.openSettings("notion")} title="Conectar Notion">
        <PageIcon />
        notion
      </button>
    );
  }

  const sync = async () => {
    setSyncing(true);
    try {
      setStatus(await api.syncNotion(call.id));
    } catch (e) {
      onError(e);
    } finally {
      setSyncing(false);
    }
  };
  const setOwn = async (page: NotionPage | null) => {
    try {
      setStatus(await api.setCallNotion(call.id, page));
    } catch (e) {
      onError(e);
    }
  };

  const destination = status?.destination ?? null;
  const synced = status?.synced ?? null;
  const syncedAt = synced && syncedFormat.format(new Date(synced.at));

  return (
    <div className="notion-control" ref={root}>
      <button
        className={`ghost-button tech-label ${open ? "active" : ""}`}
        onClick={() => setOpen((o) => !o)}
        title="En qué página de Notion va"
      >
        <PageIcon />
        <span className="notion-destination">{destination ? destination.page.title : "elegir página"}</span>
        <ChevronIcon className="chevron-down" />
      </button>
      <button
        className="ghost-button tech-label"
        onClick={sync}
        disabled={!destination || syncing}
        title={
          !destination
            ? "Elegí antes en qué página va"
            : syncedAt
              ? `Actualiza su página en «${destination.page.title}». Última vez: ${syncedAt}`
              : `Crea su página en «${destination.page.title}»`
        }
      >
        <SyncIcon className={syncing ? "spinning" : ""} />
        {syncing ? "sincronizando" : "sync"}
      </button>
      {synced && (
        <button className="ghost-button tech-label" onClick={() => openUrl(synced.url)} title="Abrir en Notion">
          <ExternalIcon />
        </button>
      )}

      {open && (
        <div className="notion-panel" role="dialog" aria-label="Página de Notion">
          <section className="notion-panel-section">
            <p className="notion-panel-label tech-label">va dentro de</p>
            {destination ? (
              <>
                <PageChip page={destination.page} />
                <p className="field-hint">
                  Por {FROM[destination.level]}
                  {destination.level === "folder" && call.folder ? ` (${call.folder})` : ""}.
                </p>
              </>
            ) : (
              <p className="field-hint">
                {call.folder ? `La carpeta ${call.folder} no tiene página` : "No hay página por defecto"}: elegila en
                ajustes o elegí una para esta llamada.
              </p>
            )}
          </section>
          <section className="notion-panel-section">
            <p className="notion-panel-label tech-label">solo esta llamada</p>
            <PagePicker
              page={status?.call_parent ?? null}
              placeholder="pegá el link de otra página"
              onChange={setOwn}
            />
          </section>
          <button
            className="ghost-button tech-label notion-panel-settings"
            onClick={() => api.openSettings("notion")}
          >
            páginas de las carpetas
            <ChevronIcon />
          </button>
        </div>
      )}
    </div>
  );
}

function PageChip({ page }: { page: NotionPage }) {
  return (
    <button className="page-chip" onClick={() => openUrl(page.url)} title="Abrir en Notion">
      <PageIcon />
      <span className="page-chip-title">{page.title}</span>
    </button>
  );
}

const syncedFormat = new Intl.DateTimeFormat("es", {
  day: "numeric",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});
