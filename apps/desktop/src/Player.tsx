// The call's audio, pinned to the bottom of the call view. The transcript follows it, and
// clicking the transcript moves it.
import { useCallback, useEffect, useImperativeHandle, useRef, useState, type PointerEvent, type Ref } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, type CallId } from "./api";
import { formatTimestamp } from "./format";
import { usePersistentState } from "./ui";

export interface PlayerHandle {
  /** Jumps to `ms` and plays from there. */
  seek: (ms: number) => void;
}

interface Props {
  callId: CallId;
  ref?: Ref<PlayerHandle>;
  /** The position, many times a second while playing. */
  onTime: (ms: number) => void;
}

const RATES = [1, 1.25, 1.5, 2];
const SKIP_MS = 15_000;

export function Player({ callId, ref, onTime }: Props) {
  const audio = useRef<HTMLAudioElement>(null);
  const [src, setSrc] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [rate, setRate] = usePersistentState("player.rate", 1);

  // Calls processed before the player existed get their audio made now: a few seconds.
  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    setError(null);
    setTime(0);
    setDuration(0);
    api.playback(callId).then(
      (path) => !cancelled && setSrc(convertFileSrc(path)),
      (e) => !cancelled && setError(String(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [callId]);

  useEffect(() => {
    if (audio.current) audio.current.playbackRate = rate;
  }, [rate, src]);

  const report = useCallback(
    (ms: number) => {
      setTime(ms);
      onTime(ms);
    },
    [onTime],
  );

  // Every frame while playing: `timeupdate` alone moves the playhead in visible jumps.
  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => {
      if (audio.current) report(audio.current.currentTime * 1000);
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing, report]);

  const seek = useCallback(
    (ms: number, play = true) => {
      const a = audio.current;
      if (!a) return;
      a.currentTime = Math.max(0, ms) / 1000;
      report(a.currentTime * 1000);
      if (play) void a.play();
    },
    [report],
  );
  useImperativeHandle(ref, () => ({ seek }), [seek]);

  const toggle = useCallback(() => {
    const a = audio.current;
    if (a) void (a.paused ? a.play() : a.pause());
  }, []);

  const skip = useCallback(
    (delta: number) => {
      const a = audio.current;
      if (a) seek(a.currentTime * 1000 + delta, !a.paused);
    },
    [seek],
  );

  // Space plays and pauses, arrows skip, the way every player does. Not while typing, and
  // not on a focused control, which handles its own keys.
  useEffect(() => {
    if (!src) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if ((e.target as HTMLElement).closest("input, textarea, select, button, [contenteditable]")) return;
      if (e.key === " ") toggle();
      else if (e.key === "ArrowLeft") skip(-5000);
      else if (e.key === "ArrowRight") skip(5000);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [src, toggle, skip]);

  // Dragging along the track scrubs; the pointer is captured so it can leave the track.
  const track = useRef<HTMLDivElement>(null);
  const scrubTo = (e: PointerEvent) => {
    const rect = track.current?.getBoundingClientRect();
    if (!rect || !duration) return;
    const fraction = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    seek(fraction * duration, !audio.current?.paused);
  };

  if (error) {
    return (
      <div className="player">
        <span className="player-note tech-label">audio no disponible</span>
        <span className="player-error">{error}</span>
      </div>
    );
  }

  const fraction = duration ? Math.min(1, time / duration) : 0;
  const ready = src != null && duration > 0;

  return (
    <div className="player">
      {src && (
        <audio
          ref={audio}
          src={src}
          preload="metadata"
          onLoadedMetadata={(e) => setDuration(e.currentTarget.duration * 1000)}
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          onEnded={() => setPlaying(false)}
          onSeeked={(e) => report(e.currentTarget.currentTime * 1000)}
        />
      )}

      {!ready ? (
        <span className="player-note tech-label">
          <span className="spinner" />
          preparando el audio
        </span>
      ) : (
        <>
          <button className="player-play" onClick={toggle} aria-label={playing ? "Pausa" : "Reproducir"}>
            {playing ? <PauseIcon /> : <PlayIcon />}
          </button>
          <button className="ghost-button tech-label" onClick={() => skip(-SKIP_MS)} title="Atrás 15 s (←: 5 s)">
            −15
          </button>
          <button className="ghost-button tech-label" onClick={() => skip(SKIP_MS)} title="Adelante 15 s (→: 5 s)">
            +15
          </button>
          <time className="player-time">{formatTimestamp(time)}</time>
          <div
            ref={track}
            className="player-track"
            role="slider"
            aria-label="Posición"
            aria-valuemin={0}
            aria-valuemax={Math.round(duration / 1000)}
            aria-valuenow={Math.round(time / 1000)}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
              scrubTo(e);
            }}
            onPointerMove={(e) => e.currentTarget.hasPointerCapture(e.pointerId) && scrubTo(e)}
          >
            <span className="player-fill" style={{ width: `${fraction * 100}%` }} />
            <span className="player-head" style={{ left: `${fraction * 100}%` }} />
          </div>
          <time className="player-time muted">{formatTimestamp(duration)}</time>
          <button
            className="ghost-button tech-label player-rate"
            onClick={() => setRate(RATES[(RATES.indexOf(rate) + 1) % RATES.length])}
            title="Velocidad"
          >
            {rate}×
          </button>
        </>
      )}
    </div>
  );
}

const PlayIcon = () => (
  <svg className="icon" viewBox="0 0 16 16" aria-hidden>
    <path d="M4.5 3v10l8.5-5z" fill="currentColor" />
  </svg>
);

const PauseIcon = () => (
  <svg className="icon" viewBox="0 0 16 16" aria-hidden>
    <path d="M4 3h3v10H4zM9 3h3v10H9z" fill="currentColor" />
  </svg>
);
