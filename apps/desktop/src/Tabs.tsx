import { useState, type KeyboardEvent, type ReactNode } from "react";
import { CheckIcon, CopyIcon } from "./icons";

export interface Tab {
  id: string;
  label: string;
  /** A live fact about the tab, shown on the rail's right while it is open. */
  meta?: ReactNode;
  actions?: ReactNode;
  content: ReactNode;
}

interface Props {
  tabs: Tab[];
  active: Tab;
  onSelect: (id: string) => void;
}

/**
 * The tabs of a page on a mono rail: names on the left, the open tab's facts and
 * actions on the right. The open tab is marked by a rule under it, never a filled
 * shape. The rail stays pinned while the content scrolls under it.
 */
export function TabRail({ tabs, active, onSelect }: Props) {
  // Arrow keys move between tabs, the way a native tab bar does.
  const onKeyDown = (e: KeyboardEvent) => {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (step === 0) return;
    const next = tabs[(tabs.indexOf(active) + step + tabs.length) % tabs.length];
    onSelect(next.id);
    document.getElementById(`tab-${next.id}`)?.focus();
  };

  return (
    <div className="tab-rail">
      <div className="tab-list" role="tablist" onKeyDown={onKeyDown}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            id={`tab-${tab.id}`}
            className="tab tech-label"
            role="tab"
            aria-selected={tab === active}
            aria-controls="tab-panel"
            tabIndex={tab === active ? 0 : -1}
            onClick={() => onSelect(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div className="tab-side">
        {active.meta && <span className="tab-meta tech-label">{active.meta}</span>}
        {active.actions}
      </div>
    </div>
  );
}

export function TabPanel({ tab }: { tab: Tab }) {
  return (
    <div id="tab-panel" role="tabpanel" aria-labelledby={`tab-${tab.id}`}>
      {tab.content}
    </div>
  );
}

export function CopyButton({ text, label = "copiar" }: { text: () => string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(text());
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <button className="ghost-button tech-label" onClick={copy}>
      {copied ? <CheckIcon /> : <CopyIcon />}
      {copied ? "copiado" : label}
    </button>
  );
}
