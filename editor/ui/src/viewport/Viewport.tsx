// The viewport: Godot's tool strip and View menu above the picture. In a browser it shows a
// streamed `capture.screenshot` (ADR-0043 approach D); in the native shell the slot is reported
// to Rust, which places the Metal view over it (approach A).
import { useCallback, useEffect, useRef, useState } from "react";

import type { EditorSession } from "../engine/session";

type Tool = "select" | "move";

interface Capture {
  image: { mime_type: string; data_base64: string };
}

export function Viewport({ session, world }: { session: EditorSession; world: "edit" | "play" }) {
  const [tool, setTool] = useState<Tool>("select");
  const [src, setSrc] = useState<string>();
  const [status, setStatus] = useState<string>();
  const slot = useRef<HTMLDivElement>(null);
  const capture = useCallback(() => {
    const box = slot.current?.getBoundingClientRect();
    const width = Math.max(16, Math.round(box?.width ?? 640));
    const height = Math.max(16, Math.round(box?.height ?? 360));
    session.call<Capture>("capture.screenshot", { width, height, world }).then(
      (c) => {
        setSrc(`data:${c.image.mime_type};base64,${c.image.data_base64}`);
        setStatus(undefined);
      },
      (e: unknown) => setStatus(String(e)),
    );
  }, [session, world]);
  useEffect(capture, [capture]);
  useEffect(() => session.onEvent(capture), [session, capture]);
  useEffect(() => {
    const keys = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      if (e.key === "q") setTool("select");
      if (e.key === "w") setTool("move");
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, []);
  return (
    <div style={{ display: "grid", gridTemplateRows: "28px 1fr", height: "100%", minHeight: 0 }}>
      <div
        role="toolbar"
        aria-label="Viewport tools"
        style={{ display: "flex", gap: 4, alignItems: "center", padding: "0 6px", background: "#262626" }}
      >
        <button aria-pressed={tool === "select"} onClick={() => setTool("select")} title="Select (Q)">
          Select
        </button>
        <button aria-pressed={tool === "move"} onClick={() => setTool("move")} title="Move (W)">
          Move
        </button>
        <span style={{ flex: 1 }} />
        <button onClick={capture} title="Refresh the streamed view">
          View ▾
        </button>
      </div>
      <div ref={slot} id="viewport-slot" style={{ position: "relative", minHeight: 0, background: "#111" }}>
        {src && (
          <img
            alt="Viewport"
            src={src}
            style={{ width: "100%", height: "100%", objectFit: "contain", display: "block" }}
          />
        )}
        {status && <p style={{ position: "absolute", top: 8, left: 8, color: "#f88" }}>{status}</p>}
        <span style={{ position: "absolute", top: 6, left: 8, opacity: 0.7 }}>
          {world === "play" ? "Playing" : "Perspective"}
        </span>
      </div>
    </div>
  );
}
