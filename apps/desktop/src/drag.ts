// Dragging calls onto folders in the sidebar. Done with pointer events: the webview's
// HTML drag and drop competes with the window's file drop (importing recordings).
import { useCallback, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { Call } from "./api";

/** Where a call would land: a folder name, `null` for "no folder", `undefined` for nowhere. */
export type DropTarget = string | null | undefined;

export interface Drag {
  call: Call;
  x: number;
  y: number;
  target: DropTarget;
}

/** Drop targets are elements with `data-drop`, and `data-folder` unless they mean "no folder". */
function targetAt(x: number, y: number): DropTarget {
  const element = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
  if (!element) return undefined;
  return element.dataset.folder ?? null;
}

export function useCallDrag(onDrop: (call: Call, folder: string | null) => void) {
  const [drag, setDrag] = useState<Drag | null>(null);
  const dropped = useRef(false);

  const start = useCallback(
    (call: Call, event: ReactPointerEvent) => {
      if (event.button !== 0) return;
      const origin = { x: event.clientX, y: event.clientY };
      let active = false;
      let target: DropTarget;

      const move = (e: PointerEvent) => {
        if (!active && Math.hypot(e.clientX - origin.x, e.clientY - origin.y) < 5) return;
        active = true;
        target = targetAt(e.clientX, e.clientY);
        setDrag({ call, x: e.clientX, y: e.clientY, target });
      };
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        if (!active) return;
        setDrag(null);
        // The click that follows the drop must not select the call.
        dropped.current = true;
        setTimeout(() => (dropped.current = false));
        if (target !== undefined && target !== (call.folder ?? null)) onDrop(call, target);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    },
    [onDrop],
  );

  const justDropped = useCallback(() => dropped.current, []);

  return { drag, start, justDropped };
}
