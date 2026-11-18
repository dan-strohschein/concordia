// Entry point: connects to the engine (URL and token from the native shell, or `?engine=` in a
// browser) and renders the editor.
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { EngineClient, websocketTransport } from "./api/client";
import { EditorSession } from "./engine/session";
import { Editor } from "./shell/Editor";

declare global {
  interface Window {
    /** Injected by the native shell: `ws://127.0.0.1:<port>/?token=…`. */
    __CONCORDIA_ENGINE__?: string;
  }
}

function engineUrl(): string | undefined {
  return window.__CONCORDIA_ENGINE__ ?? new URLSearchParams(window.location.search).get("engine") ?? undefined;
}

function App() {
  const [url] = useState(engineUrl);
  const [session, setSession] = useState<EditorSession>();
  const [error, setError] = useState<string>();
  useEffect(() => {
    if (url === undefined) return;
    websocketTransport(url)
      .then((t) => EditorSession.open(new EngineClient(t)))
      .then(setSession, (e: unknown) => setError(String(e)));
  }, [url]);
  if (url === undefined) {
    return (
      <p style={{ color: "#f88", padding: 16, font: "13px system-ui" }}>
        No engine to connect to: open the editor from the native shell, or pass ?engine=ws://…
      </p>
    );
  }
  if (error) return <p style={{ color: "#f88", padding: 16, font: "13px system-ui" }}>{error}</p>;
  if (!session) return <p style={{ padding: 16, font: "13px system-ui" }}>Connecting…</p>;
  return <Editor session={session} />;
}

const root = document.getElementById("root");
if (root) {
  document.body.style.margin = "0";
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
