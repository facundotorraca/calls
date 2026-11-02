// The editor of a call's notes. They are Markdown, and an image is a line of them where it was
// added: `![12:34](images/2026-11-02_153012.png)`, the alt being the moment of the call (see
// crates/core/src/notes.rs). Here the text reads as text and the images as images: blocks of
// text with the images between them, typed in and saved as the one Markdown text.
import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type ClipboardEvent, type KeyboardEvent, type Ref } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { api, IMAGE_EXTENSIONS, type CallId, type NoteImage } from "./api";
import { formatTimestamp } from "./format";

/** How long typing has to stop before the text is saved. */
const SAVE_AFTER_MS = 600;

const IMAGE_LINE = /^!\[([^\]]*)\]\(images\/([^)\s]+)\)$/;

type Block =
  | { key: number; kind: "text"; text: string }
  | { key: number; kind: "image"; line: string; file: string; atMs: number | null };

/** Where the caret is: a text block and the selection in it. */
interface Caret {
  key: number;
  start: number;
  end: number;
}

let keys = 0;
const textBlock = (text: string): Block => ({ key: keys++, kind: "text", text });

function imageBlock(line: string): Block | null {
  const match = IMAGE_LINE.exec(line.trim());
  return match && { key: keys++, kind: "image", line: line.trim(), file: match[2], atMs: parseTimestamp(match[1]) };
}

/** Text blocks and images, always taking turns and starting and ending with text, so there is
 * somewhere to type before and after every image. */
function parse(text: string): Block[] {
  const blocks: Block[] = [];
  let lines: string[] = [];
  for (const line of text.split("\n")) {
    const image = imageBlock(line);
    if (!image) {
      lines.push(line);
      continue;
    }
    blocks.push(textBlock(lines.join("\n")), image);
    lines = [];
  }
  blocks.push(textBlock(lines.join("\n")));
  return blocks;
}

/** Back to Markdown. Empty text blocks are only room to type: they leave nothing. */
function serialize(blocks: Block[]): string {
  return blocks
    .filter((b) => b.kind === "image" || b.text !== "")
    .map((b) => (b.kind === "image" ? b.line : b.text))
    .join("\n");
}

const imageLine = (image: NoteImage) =>
  `![${image.at_ms != null ? formatTimestamp(image.at_ms) : ""}](images/${image.file})`;

/** "12:34" or "1:02:03" as milliseconds; `null` if it is not one. */
function parseTimestamp(text: string): number | null {
  if (!/^\d+(:\d{2}){1,2}$/.test(text)) return null;
  return text.split(":").reduce((total, part) => total * 60 + Number(part), 0) * 1000;
}

/** The images in pasted content: a screenshot copied with ⌃⇧⌘4, an image copied in a browser.
 * None if there is text too: copied text can bring a picture of itself along. */
function pastedImages(e: ClipboardEvent): File[] {
  if (e.clipboardData.types.includes("text/plain")) return [];
  return [...e.clipboardData.files].filter((file) => file.type.startsWith("image/"));
}

export interface NoteEditorHandle {
  /** Adds image files (dropped on the notes) where the caret was. */
  attachFiles: (paths: string[]) => Promise<void>;
  /** Asks for image files and adds them where the caret was. */
  pick: () => Promise<void>;
}

interface Props {
  callId: CallId;
  /** The call's folder, where its images are; images show once it is known. */
  dir: string | null;
  initial: string;
  onState: (state: "saved" | "saving" | null) => void;
  onError: (error: unknown) => void;
  /** Small, for the island: thumbnails, and the box scrolls instead of growing. */
  compact?: boolean;
  placeholder?: string;
  /** Focused when it shows, with the caret after the last word. */
  autoFocus?: boolean;
  onEscape?: () => void;
  /** Plays the call from an image's moment. */
  onSeek?: (ms: number) => void;
  ref?: Ref<NoteEditorHandle>;
}

/** Saved a moment after typing stops, at once when an image goes in or out, and on leaving the
 * call. Text saved from another window (the island's notes) comes in while this one is not
 * being typed in. */
export function NoteEditor({
  callId,
  dir,
  initial,
  onState,
  onError,
  compact = false,
  placeholder = "Lo que no querés olvidar de esta llamada…",
  autoFocus = false,
  onEscape,
  onSeek,
  ref,
}: Props) {
  const [blocks, setBlocks] = useState(() => parse(initial));
  const [shown, setShown] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const areas = useRef(new Map<number, HTMLTextAreaElement>());
  const current = useRef(blocks);
  current.current = blocks;
  const caret = useRef<Caret | null>(null);
  const focusNext = useRef<{ key: number; at: number } | null>(null);
  const pending = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const flush = () => {
    clearTimeout(timer.current);
    const value = pending.current;
    if (value == null) return;
    pending.current = null;
    api.saveNotesText(callId, value).then(
      () => pending.current == null && onState("saved"),
      (e) => {
        onState(null);
        onError(e);
      },
    );
  };
  const flushRef = useRef(flush);
  flushRef.current = flush;
  // Leaving the call (or the tab) saves what is left. Mounted once per call.
  useEffect(() => () => flushRef.current(), []);

  const change = (next: Block[], now = false) => {
    setBlocks(next);
    pending.current = serialize(next);
    onState("saving");
    clearTimeout(timer.current);
    if (now) flush();
    else timer.current = setTimeout(flush, SAVE_AFTER_MS);
  };

  useEffect(() => {
    const editing = pending.current != null || root.current?.contains(document.activeElement);
    if (!editing && initial !== serialize(current.current)) setBlocks(parse(initial));
  }, [initial]);

  const focusAt = (key: number, at: number) => {
    const area = areas.current.get(key);
    area?.focus();
    area?.setSelectionRange(at, at);
  };

  // Blocks just put in by an image going in or out take the caret once they are there.
  useLayoutEffect(() => {
    const target = focusNext.current;
    focusNext.current = null;
    if (target) focusAt(target.key, target.at);
  });

  useEffect(() => {
    const last = current.current[current.current.length - 1];
    if (autoFocus && last.kind === "text") focusAt(last.key, last.text.length);
  }, [autoFocus]);

  /** Puts the images where the caret was (or at the end), each on its own line. */
  const insert = (images: NoteImage[]) => {
    if (images.length === 0) return;
    const now = current.current;
    let index = now.findIndex((b) => b.key === caret.current?.key);
    if (index < 0) index = now.length - 1;
    const block = now[index];
    if (block.kind !== "text") return;
    const { start, end } = caret.current?.key === block.key ? caret.current : { start: block.text.length, end: block.text.length };
    const added: Block[] = [{ ...block, text: block.text.slice(0, start).replace(/\n$/, "") }];
    for (const image of images) added.push(imageBlock(imageLine(image))!, textBlock(""));
    const after = added[added.length - 1];
    if (after.kind === "text") after.text = block.text.slice(end).replace(/^\n/, "");
    caret.current = { key: after.key, start: 0, end: 0 };
    focusNext.current = { key: after.key, at: 0 };
    change([...now.slice(0, index), ...added, ...now.slice(index + 1)], true);
  };

  /** Takes an image out, joining the text around it, and deletes its file. */
  const remove = (index: number) => {
    const now = current.current;
    const [before, image, after] = now.slice(index - 1, index + 2);
    if (before?.kind !== "text" || image.kind !== "image" || after?.kind !== "text") return;
    const text = [before.text, after.text].filter(Boolean).join("\n");
    focusNext.current = { key: before.key, at: before.text.length };
    change([...now.slice(0, index - 1), { ...before, text }, ...now.slice(index + 2)], true);
    api.removeImage(callId, image.file).catch(onError);
  };

  const attach = (adding: Promise<NoteImage[]>) =>
    adding.then(insert).catch((e) => {
      onError(e);
      throw e;
    });

  useImperativeHandle(ref, () => ({
    attachFiles: (paths) => attach(api.attachImageFiles(callId, paths)),
    pick: async () => {
      const paths = await open({ multiple: true, filters: [{ name: "Imágenes", extensions: IMAGE_EXTENSIONS }] });
      if (paths?.length) await attach(api.attachImageFiles(callId, paths));
    },
  }));

  const paste = (e: ClipboardEvent) => {
    const images = pastedImages(e);
    if (images.length === 0) return;
    e.preventDefault();
    void attach(Promise.all(images.map((image) => api.attachImage(callId, image)))).catch(() => {});
  };

  /** Up from the first line and down from the last go on to the text across an image. */
  const keyDown = (e: KeyboardEvent<HTMLTextAreaElement>, index: number) => {
    if (e.key === "Escape" && onEscape) return onEscape();
    const area = e.currentTarget;
    const up = e.key === "ArrowUp" && !area.value.slice(0, area.selectionStart).includes("\n");
    const down = e.key === "ArrowDown" && !area.value.slice(area.selectionEnd).includes("\n");
    const next = up ? current.current[index - 2] : down ? current.current[index + 2] : undefined;
    if (!next || next.kind !== "text") return;
    e.preventDefault();
    focusAt(next.key, up ? next.text.length : 0);
  };

  const remember = (key: number, area: HTMLTextAreaElement) => {
    caret.current = { key, start: area.selectionStart, end: area.selectionEnd };
  };

  const empty = blocks.length === 1 && blocks[0].kind === "text" && blocks[0].text === "";
  const src = (file: string) => convertFileSrc(`${dir}/images/${file}`);

  return (
    <div
      ref={root}
      className={`note-editor ${compact ? "compact" : ""}`}
      onPaste={paste}
      onMouseDown={(e) => {
        // A click on the page around the text writes at its end.
        if (e.target !== root.current) return;
        e.preventDefault();
        const last = current.current[current.current.length - 1];
        if (last.kind === "text") focusAt(last.key, last.text.length);
      }}
    >
      {blocks.map((block, index) =>
        block.kind === "text" ? (
          <TextBlock
            key={block.key}
            text={block.text}
            placeholder={empty ? placeholder : undefined}
            areaRef={(el) => (el ? areas.current.set(block.key, el) : areas.current.delete(block.key))}
            onChange={(text, area) => {
              remember(block.key, area);
              change(current.current.map((b) => (b.key === block.key ? { ...b, text } : b)));
            }}
            onCaret={(area) => remember(block.key, area)}
            onBlur={flush}
            onKeyDown={(e) => keyDown(e, index)}
          />
        ) : (
          <figure key={block.key} className="note-image">
            {dir && (
              <button
                className="note-image-thumb"
                onClick={() => !compact && setShown(block.file)}
                title={compact ? undefined : "Ver entera"}
              >
                <img src={src(block.file)} alt={block.atMs != null ? formatTimestamp(block.atMs) : ""} />
              </button>
            )}
            <figcaption className="note-image-caption">
              {block.atMs != null && (
                <button
                  className="mark-time"
                  onClick={() => onSeek?.(block.atMs!)}
                  disabled={!onSeek}
                  title={onSeek ? "Escuchar desde acá" : undefined}
                >
                  <span className="mark-square" />
                  {formatTimestamp(block.atMs)}
                </button>
              )}
              <button className="ghost-button danger note-image-remove" onClick={() => remove(index)} title="Quitar la imagen">
                ×
              </button>
            </figcaption>
          </figure>
        ),
      )}
      {shown && <ImageViewer src={src(shown)} onClose={() => setShown(null)} />}
    </div>
  );
}

/** A run of text between images: a textarea as tall as its text. */
function TextBlock({
  text,
  placeholder,
  areaRef,
  onChange,
  onCaret,
  onBlur,
  onKeyDown,
}: {
  text: string;
  placeholder?: string;
  areaRef: (el: HTMLTextAreaElement | null) => void;
  onChange: (text: string, area: HTMLTextAreaElement) => void;
  onCaret: (area: HTMLTextAreaElement) => void;
  onBlur: () => void;
  onKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [text]);

  return (
    <textarea
      ref={(el) => {
        area.current = el;
        areaRef(el);
      }}
      className="note-editor-text"
      rows={1}
      value={text}
      placeholder={placeholder}
      spellCheck
      onChange={(e) => onChange(e.target.value, e.target)}
      onSelect={(e) => onCaret(e.currentTarget)}
      onBlur={onBlur}
      onKeyDown={onKeyDown}
    />
  );
}

function ImageViewer({ src, onClose }: { src: string; onClose: () => void }) {
  useEffect(() => {
    const close = (e: globalThis.KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onClose]);
  return (
    <div className="image-viewer" onClick={onClose}>
      <img src={src} alt="" />
    </div>
  );
}
