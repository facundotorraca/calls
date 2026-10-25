// The settings window (⌘,): general preferences and the AI models, which ones are on disk
// and which ones the app uses.
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  api,
  onDownload,
  onSettingsChanged,
  type DownloadTarget,
  type Folder,
  type LocalModel,
  type OllamaModel,
  type Settings as SettingsData,
  type SettingsPage as Page,
} from "./api";
import { formatBytes } from "./format";
import { ExternalIcon, FinderIcon, PageIcon, SlidersIcon, SparklesIcon, TextIcon } from "./icons";
import { PagePicker } from "./Notion";
import { usePersistentState } from "./ui";


const PAGES: { id: Page; label: string; icon: ReactNode; group?: string }[] = [
  { id: "general", label: "General", icon: <SlidersIcon /> },
  { id: "transcription", label: "Transcripción", icon: <TextIcon />, group: "ia" },
  { id: "summary", label: "Resumen", icon: <SparklesIcon />, group: "ia" },
  { id: "notion", label: "Notion", icon: <PageIcon />, group: "integraciones" },
];

const isPage = (page: string): page is Page => PAGES.some((p) => p.id === page);

export function Settings() {
  const [page, setPage] = usePersistentState<Page>("settings.page", "general");

  // Opened (or brought up) from somewhere that needs a page: index.html#notion, or an event.
  useEffect(() => {
    const asked = location.hash.slice(1);
    if (isPage(asked)) setPage(asked);
    const unlisten = getCurrentWebviewWindow().listen<string>("settings_page", (e) => isPage(e.payload) && setPage(e.payload));
    return () => void unlisten.then((fn) => fn());
  }, [setPage]);
  const [settings, save, error] = useSettings();
  const downloads = useDownloads();

  return (
    <div className="settings">
      <aside className="sidebar">
        <header className="sidebar-header">
          <h1 className="wordmark">ajustes</h1>
        </header>
        <nav className="settings-nav">
          {PAGES.map((p, i) => (
            <div key={p.id}>
              {p.group && PAGES[i - 1]?.group !== p.group && <p className="group-label">{p.group}</p>}
              <button className={`nav-item ${page === p.id ? "active" : ""}`} onClick={() => setPage(p.id)}>
                {p.icon}
                <span className="nav-label">{p.label}</span>
              </button>
            </div>
          ))}
        </nav>
      </aside>

      <main className="content">
        {error && (
          <div className="banner error">
            <div className="banner-rail tech-label">
              <span>no se pudo guardar</span>
            </div>
            <div className="banner-body">{error}</div>
          </div>
        )}
        {settings &&
          (page === "general" ? (
            <GeneralPage settings={settings} save={save} />
          ) : page === "transcription" ? (
            <TranscriptionPage settings={settings} save={save} downloads={downloads} />
          ) : page === "summary" ? (
            <SummaryPage settings={settings} save={save} downloads={downloads} />
          ) : (
            <NotionSettingsPage settings={settings} save={save} />
          ))}
      </main>
    </div>
  );
}

type Save = (patch: Partial<SettingsData>) => void;

/** The settings, saved on every change and kept in sync with other windows. */
function useSettings(): [SettingsData | null, Save, string | null] {
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.getSettings().then(setSettings);
    const unlisten = onSettingsChanged(() => api.getSettings().then(setSettings));
    return () => void unlisten.then((fn) => fn());
  }, []);

  const save = useCallback(
    (patch: Partial<SettingsData>) => {
      if (!settings) return;
      const next = { ...settings, ...patch };
      setSettings(next);
      api.updateSettings(next).then(
        () => setError(null),
        (e) => {
          setError(String(e));
          setSettings(settings);
        },
      );
    },
    [settings],
  );

  return [settings, save, error];
}

const key = (t: DownloadTarget) => `${t.kind}:${t.name}`;

interface Downloads {
  /** Fraction done of each running download, by `kind:name`. */
  progress: Record<string, number>;
  /** Why the last download of each failed. */
  errors: Record<string, string>;
  /** Bumped when a download finishes, to reload the lists. */
  finished: number;
  start: (target: DownloadTarget, token?: string) => void;
  cancel: (target: DownloadTarget) => void;
}

/** Downloads run in the app, not in this window: closing it does not stop them. */
function useDownloads(): Downloads {
  const [progress, setProgress] = useState<Record<string, number>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [finished, setFinished] = useState(0);

  useEffect(() => {
    api.downloads().then((running) => setProgress(Object.fromEntries(running.map((d) => [key(d), d.fraction]))));
    const unlisten = onDownload((event) => {
      const k = key(event);
      if (event.state === "progress") {
        setProgress((p) => ({ ...p, [k]: event.fraction }));
        return;
      }
      setProgress(({ [k]: _, ...rest }) => rest);
      if (event.state === "failed") setErrors((e) => ({ ...e, [k]: event.error }));
      setFinished((n) => n + 1);
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  const start = useCallback((target: DownloadTarget, token?: string) => {
    const k = key(target);
    setErrors(({ [k]: _, ...rest }) => rest);
    setProgress((p) => ({ ...p, [k]: 0 }));
    api.startDownload(target, token);
  }, []);

  return { progress, errors, finished, start, cancel: api.cancelDownload };
}

// ── Pages ────────────────────────────────────────────────────────────────

function GeneralPage({ settings, save }: { settings: SettingsData; save: Save }) {
  return (
    <SettingsPage title="general">
      <Group label="vos">
        <Field label="Tu nombre" hint="Cómo aparecés en las transcripciones y los resúmenes, en vez de «Yo».">
          <TextInput
            value={settings.user_name}
            placeholder="Yo"
            optional
            onCommit={(user_name) => save({ user_name })}
          />
        </Field>
        <MyVoice />
      </Group>

      <Group label="grabación">
        <Field label="Detectar llamadas" hint="Muestra la isla cuando Zoom, Meet o Teams empiezan a usar el micrófono.">
          <Switch checked={settings.detect_calls} onChange={(detect_calls) => save({ detect_calls })} />
        </Field>
        <Field
          label="Grabar automáticamente"
          hint="Empieza a grabar sin preguntar cuando detecta una llamada."
          disabled={!settings.detect_calls}
        >
          <Switch
            checked={settings.auto_record}
            disabled={!settings.detect_calls}
            onChange={(auto_record) => save({ auto_record })}
          />
        </Field>
      </Group>

      <Group label="biblioteca">
        <Field label="Carpeta" hint={<code>{settings.library_dir}</code>}>
          <button className="ghost-button tech-label" onClick={() => revealItemInDir(settings.library_dir)}>
            <FinderIcon />
            finder
          </button>
        </Field>
      </Group>
    </SettingsPage>
  );
}

/** The user's voice, learnt from the speakers they mark as "soy yo". */
function MyVoice() {
  const [samples, setSamples] = useState<number | null>(null);

  // Learnt in the main window: check again when coming back here.
  useEffect(() => {
    const load = () => void api.myVoice().then(setSamples);
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, []);

  const forget = async () => {
    const yes = await ask("¿Olvidar tu voz?\n\nLa app deja de reconocerte hasta que vuelvas a marcar «soy yo».", {
      title: "Olvidar tu voz",
      kind: "warning",
      okLabel: "Olvidar",
      cancelLabel: "Cancelar",
    });
    if (yes) await api.forgetMyVoice().then(() => setSamples(null));
  };

  return (
    <Field
      label="Tu voz"
      hint={
        samples
          ? "Te reconoce sola en las llamadas importadas. Marcar «soy yo» en más llamadas la afina."
          : "En una transcripción, tocá el hablante que sos vos y elegí «soy yo»: la app aprende tu voz y te reconoce sola en las próximas llamadas importadas."
      }
    >
      {samples ? (
        <>
          <State tone="ok">aprendida{samples > 1 ? ` · ${samples} voces` : ""}</State>
          <button className="ghost-button tech-label" onClick={forget}>
            olvidar
          </button>
        </>
      ) : (
        <span className="field-hint">sin aprender</span>
      )}
    </Field>
  );
}

const CALL_LANGUAGES = [
  ["auto", "Automático"],
  ["es", "Español"],
  ["en", "Inglés"],
  ["pt", "Portugués"],
  ["fr", "Francés"],
  ["de", "Alemán"],
  ["it", "Italiano"],
];

function TranscriptionPage({
  settings,
  save,
  downloads,
}: {
  settings: SettingsData;
  save: Save;
  downloads: Downloads;
}) {
  const [models, setModels] = useState<LocalModel[]>([]);
  const [vadModels, setVadModels] = useState<LocalModel[]>([]);
  const [error, setError] = useState<string | null>(null);

  const [diarizationReady, setDiarizationReady] = useState<boolean | null>(null);

  const reload = useCallback(() => {
    api.localModels("whisper").then(setModels);
    api.localModels("vad").then(setVadModels);
    api.diarizationReady().then(setDiarizationReady);
  }, []);
  useEffect(reload, [reload, downloads.finished]);

  const remove = async (name: string) => {
    if (!(await confirmDelete(name, "Lo podés volver a descargar cuando quieras."))) return;
    api.deleteLocalModel(name).then(reload, (e) => setError(String(e)));
  };

  const active = models.find((m) => m.name === settings.whisper_model);
  const vad = vadModels.find((m) => m.name === settings.vad_model) ?? vadModels[0];
  const vadTarget = vad && ({ kind: "vad", name: vad.name } as const);

  return (
    <SettingsPage
      title="transcripción"
      status={
        active?.downloaded ? (
          <State tone="ok">whisper · {settings.whisper_model}</State>
        ) : (
          <State tone="error">falta descargar {settings.whisper_model}</State>
        )
      }
    >
      <Group label="idioma">
        <Field label="Idioma de las llamadas" hint="Automático lo detecta en cada llamada.">
          <Select value={settings.language} options={CALL_LANGUAGES} onChange={(language) => save({ language })} />
        </Field>
      </Group>

      <Group
        label="vocabulario"
        meta={settings.vocabulary.length > 0 ? `${settings.vocabulary.length} términos` : undefined}
      >
        <div className="vocabulary">
          <p className="field-hint">
            Nombres de personas, clientes, productos y siglas que Whisper escribe mal, tal como se escriben. Uno por
            línea. Se usa en las próximas transcripciones y en los resúmenes.
          </p>
          <TermsInput value={settings.vocabulary} onCommit={(vocabulary) => save({ vocabulary })} />
        </div>
      </Group>

      <Group label="modelo" meta="whisper.cpp · local">
        {error && <p className="model-error">{error}</p>}
        {models.map((model) => {
          const target = { kind: "whisper", name: model.name } as const;
          return (
            <ModelRow
              key={model.name}
              name={model.name}
              detail={model.note}
              bytes={model.bytes}
              inUse={model.name === settings.whisper_model}
              downloaded={model.downloaded}
              progress={downloads.progress[key(target)]}
              error={downloads.errors[key(target)]}
              onUse={() => save({ whisper_model: model.name })}
              onDownload={() => downloads.start(target)}
              onCancel={() => downloads.cancel(target)}
              onDelete={() => remove(model.name)}
            />
          );
        })}
      </Group>

      <Group label="hablantes" meta="pyannote · local">
        <Field
          label="Separar hablantes"
          hint="Distingue cada voz: en las grabaciones importadas, a todos; en las grabadas, a los del otro lado (vos ya sos «Yo»). Con el resumen activo, la IA les pone nombre cuando la llamada lo dice."
        >
          {diarizationReady && <State tone="ok">listo</State>}
          <Switch checked={settings.diarize} onChange={(diarize) => save({ diarize })} />
        </Field>
        {settings.diarize && diarizationReady === false && <DiarizationSetup downloads={downloads} />}
      </Group>

      <Group label="silencios">
        <Field label="Saltear silencios" hint="Detecta las partes sin voz para que Whisper no invente texto en ellas.">
          {vadTarget && settings.vad_model && !vad.downloaded ? (
            <DownloadControl
              progress={downloads.progress[key(vadTarget)]}
              bytes={vad.bytes}
              onDownload={() => downloads.start(vadTarget)}
              onCancel={() => downloads.cancel(vadTarget)}
            />
          ) : null}
          <Switch
            checked={settings.vad_model != null}
            onChange={(on) => save({ vad_model: on ? (vad?.name ?? null) : null })}
          />
        </Field>
      </Group>
    </SettingsPage>
  );
}

const DIARIZATION_MODEL = "https://huggingface.co/pyannote/speaker-diarization-community-1";
const DIARIZATION_TARGET = { kind: "diarization", name: "community-1" } as const;

/** The one time setup of the diarization model, which Hugging Face gates behind accepting
 * its conditions (free) and a token. */
function DiarizationSetup({ downloads }: { downloads: Downloads }) {
  const [token, setToken] = useState("");
  const progress = downloads.progress[key(DIARIZATION_TARGET)];
  const error = downloads.errors[key(DIARIZATION_TARGET)];

  return (
    <div className="setup">
      <p className="field-hint">
        El modelo es gratis, pero Hugging Face pide una cuenta para descargarlo. Una sola vez:
      </p>
      <ol className="setup-steps">
        <li>
          <span>Con tu cuenta iniciada, aceptá las condiciones del modelo (formulario arriba de todo).</span>
          <button className="ghost-button tech-label" onClick={() => openUrl(DIARIZATION_MODEL)}>
            <ExternalIcon />
            abrir
          </button>
        </li>
        <li>
          <span>Creá un token de lectura (Read).</span>
          <button className="ghost-button tech-label" onClick={() => openUrl("https://huggingface.co/settings/tokens")}>
            <ExternalIcon />
            abrir
          </button>
        </li>
        <li>
          <span>Pegalo acá y descargá.</span>
        </li>
      </ol>
      <form
        className="model-pull"
        onSubmit={(e) => {
          e.preventDefault();
          downloads.start(DIARIZATION_TARGET, token.trim());
          setToken("");
        }}
      >
        <input
          className="text-input"
          type="password"
          placeholder="hf_…"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          disabled={progress != null}
          spellCheck={false}
        />
        {progress != null ? (
          <DownloadControl progress={progress} />
        ) : (
          <button className="button" type="submit" disabled={!token.trim()}>
            descargar
          </button>
        )}
      </form>
      {error && <p className="model-error">{error}</p>}
      <p className="field-hint">
        El token se usa solo para esta descarga y no se guarda. La primera vez también se instala PyTorch (~1 GB): tarda
        unos minutos.
      </p>
    </div>
  );
}

function NotionSettingsPage({ settings, save }: { settings: SettingsData; save: Save }) {
  const { workspace, parent } = settings.notion;
  const [token, setToken] = useState("");
  const [folders, setFolders] = useState<Folder[]>([]);

  // Folders are made in the main window: check again when coming back here.
  const loadFolders = useCallback(() => void api.listFolders().then(setFolders), []);
  useEffect(() => {
    loadFolders();
    window.addEventListener("focus", loadFolders);
    return () => window.removeEventListener("focus", loadFolders);
  }, [loadFolders]);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const connect = async () => {
    setConnecting(true);
    setError(null);
    try {
      await api.notionConnect(token.trim());
      setToken("");
    } catch (e) {
      setError(String(e));
    } finally {
      setConnecting(false);
    }
  };

  return (
    <SettingsPage
      title="notion"
      status={workspace ? <State tone="ok">{workspace}</State> : <State tone="error">sin conectar</State>}
    >
      <Group label="conexión">
        {workspace ? (
          <Field label="Espacio de trabajo" hint="El token está guardado en el Llavero de macOS.">
            <State tone="ok">{workspace}</State>
            <button className="ghost-button tech-label" onClick={() => api.notionDisconnect()}>
              desconectar
            </button>
          </Field>
        ) : (
          <div className="setup">
            <p className="field-hint">Cada llamada se puede mandar a Notion como una página. Una sola vez:</p>
            <ol className="setup-steps">
              <li>
                <span>Creá una integración interna y copiá su secreto (empieza con ntn_).</span>
                <button
                  className="ghost-button tech-label"
                  onClick={() => openUrl("https://www.notion.so/profile/integrations")}
                >
                  <ExternalIcon />
                  abrir
                </button>
              </li>
              <li>
                <span>Pegalo acá y conectá.</span>
              </li>
              <li>
                <span>En Notion, en las páginas donde quieras las llamadas: ··· → Conexiones → tu integración.</span>
              </li>
            </ol>
            <form
              className="model-pull"
              onSubmit={(e) => {
                e.preventDefault();
                void connect();
              }}
            >
              <input
                className="text-input"
                type="password"
                placeholder="ntn_…"
                value={token}
                onChange={(e) => setToken(e.target.value)}
                disabled={connecting}
                spellCheck={false}
              />
              <button className="button" type="submit" disabled={!token.trim() || connecting}>
                {connecting ? "conectando" : "conectar"}
              </button>
            </form>
            {error && <p className="model-error">{error}</p>}
            <p className="field-hint">El token se guarda en el Llavero de macOS. Nada se manda a Notion sin que toques sync.</p>
          </div>
        )}
      </Group>

      <p className="notion-explainer field-hint">
        Cada llamada se crea en Notion como una página con su nombre, dentro de la página de su carpeta. Una llamada puede
        ir a otra página: se elige en la llamada.
      </p>

      <Group label="carpetas" meta={folders.length > 0 ? `${folders.length}` : undefined}>
        {folders.map((folder) => (
          <Field
            key={folder.name}
            label={folder.name}
            hint={folder.notion ? undefined : "Sin página: usa la por defecto."}
            disabled={!workspace}
          >
            {workspace && (
              <PagePicker
                page={folder.notion ?? null}
                onChange={(page) => api.setFolderNotion(folder.name, page).then(loadFolders)}
              />
            )}
          </Field>
        ))}
        <Field
          label="Por defecto"
          hint="Para las llamadas sin carpeta y las carpetas sin página."
          disabled={!workspace}
        >
          {workspace && (
            <PagePicker page={parent} onChange={(page) => save({ notion: { ...settings.notion, parent: page } })} />
          )}
        </Field>
      </Group>
    </SettingsPage>
  );
}

const SUMMARY_LANGUAGES = [
  ["español", "Español"],
  ["English", "Inglés"],
  ["português", "Portugués"],
];

function SummaryPage({ settings, save, downloads }: { settings: SettingsData; save: Save; downloads: Downloads }) {
  const [models, setModels] = useState<OllamaModel[] | null>(null);
  const [offline, setOffline] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pull, setPull] = useState("");
  const { llm } = settings;

  const reload = useCallback(() => {
    api.ollamaModels().then(
      (list) => {
        setModels(list);
        setOffline(null);
      },
      (e) => {
        setModels(null);
        setOffline(String(e));
      },
    );
  }, []);
  useEffect(reload, [reload, downloads.finished, llm.base_url]);

  const remove = async (name: string) => {
    if (!(await confirmDelete(name, "Ollama lo borra del disco; lo podés volver a descargar."))) return;
    api.deleteOllamaModel(name).then(reload, (e) => setError(String(e)));
  };

  const startPull = (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    downloads.start({ kind: "ollama", name: trimmed });
    setPull("");
  };

  // Rows: what Ollama has, plus what is configured or downloading but not there yet.
  const installed = models ?? [];
  const pending = [
    llm.model,
    ...Object.keys(downloads.progress)
      .filter((k) => k.startsWith("ollama:"))
      .map((k) => k.slice("ollama:".length)),
  ].filter((name, i, all) => all.indexOf(name) === i && !installed.some((m) => m.name === name));

  const ready = installed.some((m) => m.name === llm.model);

  return (
    <SettingsPage
      title="resumen"
      status={
        offline ? (
          <State tone="error">ollama no responde</State>
        ) : models && !ready ? (
          <State tone="error">falta descargar {llm.model}</State>
        ) : models ? (
          <State tone="ok">ollama · {llm.model}</State>
        ) : null
      }
    >
      <Group label="resumen">
        <Field label="Resumir las llamadas" hint="Resumen, decisiones y pendientes al terminar de transcribir.">
          <Switch checked={settings.summarize} onChange={(summarize) => save({ summarize })} />
        </Field>
        <Field label="Idioma del resumen" hint="Se escribe en este idioma, sea cual sea el de la llamada.">
          <Select
            value={settings.summary_language}
            options={SUMMARY_LANGUAGES}
            onChange={(summary_language) => save({ summary_language })}
          />
        </Field>
      </Group>

      <Group label="ollama" meta="modelos locales">
        <Field
          label="Servidor"
          hint={offline ? <span className="hint-error">{offline}</span> : "Donde corre Ollama. Abrí la app de Ollama."}
        >
          {offline && (
            <button className="ghost-button tech-label" onClick={reload}>
              reintentar
            </button>
          )}
          <TextInput value={llm.base_url} onCommit={(base_url) => save({ llm: { ...llm, base_url } })} />
        </Field>
      </Group>

      {!offline && (
        <Group label="modelo" meta={models ? `${installed.length} instalados` : undefined}>
          {error && <p className="model-error">{error}</p>}
          {installed.map((model) => {
            const target = { kind: "ollama", name: model.name } as const;
            return (
              <ModelRow
                key={model.name}
                name={model.name}
                detail={[model.details.parameter_size, model.details.quantization_level].filter(Boolean).join(" · ")}
                bytes={model.size}
                inUse={model.name === llm.model}
                downloaded
                progress={downloads.progress[key(target)]}
                error={downloads.errors[key(target)]}
                onUse={() => save({ llm: { ...llm, model: model.name } })}
                onDelete={() => remove(model.name)}
              />
            );
          })}
          {pending.map((name) => {
            const target = { kind: "ollama", name } as const;
            return (
              <ModelRow
                key={name}
                name={name}
                detail={name === llm.model ? "Elegido, pero Ollama no lo tiene" : null}
                bytes={null}
                inUse={name === llm.model}
                downloaded={false}
                progress={downloads.progress[key(target)]}
                error={downloads.errors[key(target)]}
                onDownload={() => downloads.start(target)}
                onCancel={() => downloads.cancel(target)}
              />
            );
          })}
          <form
            className="model-pull"
            onSubmit={(e) => {
              e.preventDefault();
              startPull(pull);
            }}
          >
            <input
              className="text-input"
              placeholder="Otro modelo, como en ollama pull"
              value={pull}
              onChange={(e) => setPull(e.target.value)}
              spellCheck={false}
            />
            <button className="button" type="submit" disabled={!pull.trim()}>
              descargar
            </button>
            <button
              className="ghost-button tech-label"
              type="button"
              onClick={() => openUrl("https://ollama.com/search")}
            >
              <ExternalIcon />
              explorar
            </button>
          </form>
        </Group>
      )}
    </SettingsPage>
  );
}

// ── Pieces ───────────────────────────────────────────────────────────────

function SettingsPage({ title, status, children }: { title: string; status?: ReactNode; children: ReactNode }) {
  return (
    <article className="settings-page">
      <header className="page-header">
        <h2 className="font-dot page-title">{title}</h2>
        {status && <span className="page-status tech-label">{status}</span>}
      </header>
      <div className="settings-body">{children}</div>
    </article>
  );
}

/** A hairline panel with a mono rail: the settings' one grouping shape. */
function Group({ label, meta, children }: { label: string; meta?: string; children: ReactNode }) {
  return (
    <section className="group">
      <div className="group-rail tech-label">
        <span>{label}</span>
        {meta && <span>{meta}</span>}
      </div>
      <div className="group-body">{children}</div>
    </section>
  );
}

function Field({
  label,
  hint,
  disabled,
  children,
}: {
  label: string;
  hint?: ReactNode;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={`field ${disabled ? "disabled" : ""}`}>
      <div className="field-text">
        <p className="field-label">{label}</p>
        {hint && <div className="field-hint">{hint}</div>}
      </div>
      <div className="field-control">{children}</div>
    </div>
  );
}

function State({ tone, children }: { tone: "ok" | "error"; children: ReactNode }) {
  return (
    <span className={`state ${tone}`}>
      <span className="state-mark" />
      {children}
    </span>
  );
}

function Switch({
  checked,
  disabled,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      className="switch"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="switch-knob" />
    </button>
  );
}

function Select({
  value,
  options,
  onChange,
}: {
  value: string;
  options: string[][];
  onChange: (value: string) => void;
}) {
  // A value set by hand in settings.json still shows up.
  const all = options.some(([v]) => v === value) ? options : [...options, [value, value]];
  return (
    <select className="select" value={value} onChange={(e) => onChange(e.target.value)}>
      {all.map(([v, label]) => (
        <option key={v} value={v}>
          {label}
        </option>
      ))}
    </select>
  );
}

/** Saves on Enter or when leaving the field, not on every keystroke. */
function TextInput({
  value,
  placeholder,
  optional,
  onCommit,
}: {
  value: string;
  placeholder?: string;
  /** Can be left empty. */
  optional?: boolean;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => {
    const next = draft.trim();
    if ((next || optional) && next !== value) onCommit(next);
    else setDraft(value);
  };
  return (
    <input
      className="text-input"
      value={draft}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(value);
      }}
    />
  );
}

/** One term per line (commas also split). Saves when leaving the field. */
function TermsInput({ value, onCommit }: { value: string[]; onCommit: (terms: string[]) => void }) {
  const [draft, setDraft] = useState(value.join("\n"));
  useEffect(() => setDraft(value.join("\n")), [value]);
  const commit = () => {
    const seen = new Set<string>();
    const terms = draft
      .split(/[\n,]/)
      .map((t) => t.trim())
      .filter((t) => t && !seen.has(t.toLowerCase()) && seen.add(t.toLowerCase()));
    if (terms.join("\n") !== value.join("\n")) onCommit(terms);
    else setDraft(value.join("\n"));
  };
  return (
    <textarea
      className="text-input terms"
      value={draft}
      rows={Math.min(10, Math.max(4, draft.split("\n").length + 1))}
      placeholder={"Faku\nAcme\nKubernetes"}
      spellCheck={false}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
    />
  );
}

interface ModelRowProps {
  name: string;
  detail: string | null;
  bytes: number | null;
  inUse: boolean;
  downloaded: boolean;
  /** Set while downloading. */
  progress?: number;
  error?: string;
  onUse?: () => void;
  onDownload?: () => void;
  onCancel?: () => void;
  onDelete?: () => void;
}

/** One model: name, what it is, its size, and the one thing to do with it. */
function ModelRow({
  name,
  detail,
  bytes,
  inUse,
  downloaded,
  progress,
  error,
  onUse,
  onDownload,
  onCancel,
  onDelete,
}: ModelRowProps) {
  return (
    <div className={`model-row row-tick ${inUse ? "in-use" : ""}`}>
      <div className="model-text">
        <p className="model-name">{name}</p>
        {detail && <p className="model-detail">{detail}</p>}
        {error && <p className="model-error">{error}</p>}
      </div>
      <span className="model-size">{bytes != null && formatBytes(bytes)}</span>
      <div className="model-actions">
        {progress != null ? (
          <DownloadControl progress={progress} onCancel={onCancel} />
        ) : downloaded ? (
          <>
            {!inUse && onDelete && (
              <button className="ghost-button tech-label danger" onClick={onDelete}>
                borrar
              </button>
            )}
            {inUse ? (
              <State tone="ok">en uso</State>
            ) : (
              <button className="button" onClick={onUse}>
                usar
              </button>
            )}
          </>
        ) : (
          <button className="button" onClick={onDownload}>
            descargar
          </button>
        )}
      </div>
    </div>
  );
}

function DownloadControl({
  progress,
  bytes,
  onDownload,
  onCancel,
}: {
  progress?: number;
  bytes?: number | null;
  onDownload?: () => void;
  onCancel?: () => void;
}) {
  if (progress == null) {
    return (
      <button className="button" onClick={onDownload}>
        descargar{bytes != null && ` · ${formatBytes(bytes)}`}
      </button>
    );
  }
  return (
    <span className="download">
      <span className="bar">
        <span style={{ width: `${progress * 100}%` }} />
      </span>
      <span className="download-pct tech-label">{Math.floor(progress * 100)}%</span>
      {onCancel && (
        <button className="ghost-button tech-label" onClick={onCancel}>
          cancelar
        </button>
      )}
    </span>
  );
}

const confirmDelete = (name: string, detail: string) =>
  ask(`¿Borrar ${name}?\n\n${detail}`, {
    title: "Borrar modelo",
    kind: "warning",
    okLabel: "Borrar",
    cancelLabel: "Cancelar",
  });
