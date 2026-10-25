// Small UI helpers shared by the components: persisted state and native menus.
import { useCallback, useState } from "react";
import {
  Menu,
  type CheckMenuItemOptions,
  type MenuItemOptions,
  type PredefinedMenuItemOptions,
  type SubmenuOptions,
} from "@tauri-apps/api/menu";

/** `useState` remembered across launches (in this machine's webview storage). */
export function usePersistentState<T>(key: string, initial: T): [T, (value: T) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const saved = localStorage.getItem(key);
      return saved == null ? initial : (JSON.parse(saved) as T);
    } catch {
      return initial;
    }
  });
  const set = useCallback(
    (next: T) => {
      setValue(next);
      try {
        localStorage.setItem(key, JSON.stringify(next));
      } catch {
        // Not persisted; the state still works for this session.
      }
    },
    [key],
  );
  return [value, set];
}

export type MenuEntry =
  | "separator"
  | {
      text: string;
      action?: () => void;
      checked?: boolean;
      enabled?: boolean;
      /** Shown next to the item, e.g. `CmdOrCtrl+Backspace`; the key itself is handled elsewhere. */
      accelerator?: string;
      items?: MenuEntry[];
    };

/** Shows a native context menu at the cursor. */
export async function popupMenu(entries: MenuEntry[]) {
  const menu = await Menu.new({ items: entries.map(toItem) });
  await menu.popup();
}

type ItemOptions = MenuItemOptions | CheckMenuItemOptions | SubmenuOptions | PredefinedMenuItemOptions;

function toItem(entry: MenuEntry): ItemOptions {
  if (entry === "separator") return { item: "Separator" };
  if (entry.items) return { text: entry.text, items: entry.items.map(toItem) };
  const item: CheckMenuItemOptions = {
    text: entry.text,
    enabled: entry.enabled ?? true,
    accelerator: entry.accelerator,
    action: entry.action,
  };
  // Tauri makes it a check item only when the key is present.
  if (entry.checked !== undefined) item.checked = entry.checked;
  return item;
}

/** Lowercase without accents, for searching. */
export const normalize = (text: string) =>
  text
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
