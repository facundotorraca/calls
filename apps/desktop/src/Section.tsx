import { useState, type ReactNode } from "react";
import { CheckIcon, ChevronIcon, CopyIcon } from "./icons";
import { usePersistentState } from "./ui";

interface Props {
  /** Remembers whether it is open, shared by every call. */
  id: string;
  icon: ReactNode;
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}

/** A card with a sticky header that collapses its content. */
export function Section({ id, icon, title, meta, actions, children }: Props) {
  const [open, setOpen] = usePersistentState(`section.${id}.open`, true);

  return (
    <section className={`section ${open ? "open" : ""}`}>
      <header className="section-header">
        <button className="section-toggle" onClick={() => setOpen(!open)} aria-expanded={open}>
          <ChevronIcon className="section-chevron" />
          <span className="section-icon">{icon}</span>
          <span className="section-title">{title}</span>
          {meta && <span className="section-meta">{meta}</span>}
        </button>
        {actions && <div className="section-actions">{actions}</div>}
      </header>
      <div className="section-body" inert={!open}>
        <div className="section-clip">
          <div className="section-content">{children}</div>
        </div>
      </div>
    </section>
  );
}

export function CopyButton({ text, label = "Copiar" }: { text: () => string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(text());
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <button className="ghost-button" onClick={copy}>
      {copied ? <CheckIcon /> : <CopyIcon />}
      {copied ? "Copiado" : label}
    </button>
  );
}
