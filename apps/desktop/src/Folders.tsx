import { useRef, useState, type ReactNode } from "react";
import { api, type Call } from "./api";
import type { DropTarget } from "./drag";
import { FolderIcon, InboxIcon, PlusIcon, TrayIcon } from "./icons";
import { popupMenu } from "./ui";

/** Which calls the sidebar lists. */
export type Scope = { kind: "all" } | { kind: "unfiled" } | { kind: "folder"; name: string };

export function inScope(call: Call, scope: Scope): boolean {
  if (scope.kind === "all") return true;
  if (scope.kind === "unfiled") return !call.folder;
  return call.folder === scope.name;
}

interface Props {
  folders: string[];
  calls: Call[];
  scope: Scope;
  onScope: (scope: Scope) => void;
  /** Set while a call is being dragged. */
  dropTarget?: DropTarget;
  onChanged: () => Promise<void>;
  onError: (error: unknown) => void;
}

type Editing = { kind: "new" } | { kind: "rename"; name: string } | null;

export function Folders({ folders, calls, scope, onScope, dropTarget, onChanged, onError }: Props) {
  const [editing, setEditing] = useState<Editing>(null);

  const count = (s: Scope) => calls.filter((c) => inScope(c, s)).length;
  const isScope = (s: Scope) =>
    s.kind === scope.kind && (s.kind !== "folder" || (scope.kind === "folder" && s.name === scope.name));

  const create = async (name: string) => {
    setEditing(null);
    try {
      const created = await api.createFolder(name);
      await onChanged();
      onScope({ kind: "folder", name: created });
    } catch (e) {
      onError(e);
    }
  };

  const rename = async (from: string, to: string) => {
    setEditing(null);
    if (from === to) return;
    try {
      await api.renameFolder(from, to);
      if (scope.kind === "folder" && scope.name === from) onScope({ kind: "folder", name: to.trim() });
      await onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const remove = async (name: string) => {
    try {
      await api.deleteFolder(name);
      if (scope.kind === "folder" && scope.name === name) onScope({ kind: "all" });
      await onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const folderMenu = (name: string) =>
    popupMenu([
      { text: "Renombrar", action: () => setEditing({ kind: "rename", name }) },
      "separator",
      { text: "Eliminar carpeta (las llamadas quedan)", action: () => remove(name) },
    ]);

  return (
    <nav className="folders">
      <NavItem
        icon={<TrayIcon />}
        label="Todas las llamadas"
        count={count({ kind: "all" })}
        active={isScope({ kind: "all" })}
        onClick={() => onScope({ kind: "all" })}
      />
      {folders.length > 0 && (
        <NavItem
          icon={<InboxIcon />}
          label="Sin carpeta"
          count={count({ kind: "unfiled" })}
          active={isScope({ kind: "unfiled" })}
          onClick={() => onScope({ kind: "unfiled" })}
          drop={{ folder: null, over: dropTarget === null }}
        />
      )}

      <div className="folders-header">
        <span>Carpetas</span>
        <button className="icon-button" title="Nueva carpeta" onClick={() => setEditing({ kind: "new" })}>
          <PlusIcon />
        </button>
      </div>

      {folders.map((name) =>
        editing?.kind === "rename" && editing.name === name ? (
          <NameInput key={name} initial={name} onSubmit={(to) => rename(name, to)} onCancel={() => setEditing(null)} />
        ) : (
          <NavItem
            key={name}
            icon={<FolderIcon />}
            label={name}
            count={count({ kind: "folder", name })}
            active={isScope({ kind: "folder", name })}
            onClick={() => onScope({ kind: "folder", name })}
            onDoubleClick={() => setEditing({ kind: "rename", name })}
            onContextMenu={() => folderMenu(name)}
            drop={{ folder: name, over: dropTarget === name }}
          />
        ),
      )}

      {editing?.kind === "new" && (
        <NameInput initial="Nueva carpeta" onSubmit={create} onCancel={() => setEditing(null)} />
      )}
      {folders.length === 0 && editing?.kind !== "new" && (
        <button className="folders-hint" onClick={() => setEditing({ kind: "new" })}>
          Creá una carpeta y arrastrá llamadas a ella
        </button>
      )}
    </nav>
  );
}

interface NavItemProps {
  icon: ReactNode;
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: () => void;
  drop?: { folder: string | null; over: boolean };
}

function NavItem({ icon, label, count, active, onClick, onDoubleClick, onContextMenu, drop }: NavItemProps) {
  return (
    <button
      className={`nav-item ${active ? "active" : ""} ${drop?.over ? "drop-over" : ""}`}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={(e) => {
        if (!onContextMenu) return;
        e.preventDefault();
        onContextMenu();
      }}
      data-drop={drop ? "" : undefined}
      data-folder={drop?.folder ?? undefined}
    >
      {icon}
      <span className="nav-label">{label}</span>
      <span className="nav-count">{count || ""}</span>
    </button>
  );
}

function NameInput({
  initial,
  onSubmit,
  onCancel,
}: {
  initial: string;
  onSubmit: (name: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  // Enter and then the blur of the input going away must submit once.
  const done = useRef(false);
  const finish = (submit: boolean) => {
    if (done.current) return;
    done.current = true;
    if (submit && value.trim()) onSubmit(value.trim());
    else onCancel();
  };
  return (
    <div className="nav-item editing">
      <FolderIcon />
      <input
        className="nav-input"
        value={value}
        autoFocus
        onFocus={(e) => e.target.select()}
        onChange={(e) => setValue(e.target.value)}
        onBlur={() => finish(true)}
        onKeyDown={(e) => {
          if (e.key === "Enter") finish(true);
          if (e.key === "Escape") finish(false);
        }}
      />
    </div>
  );
}
