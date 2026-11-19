// A small readout of the last frame's cost (`render.stats`), refreshed once a second while visible.
import { useEffect, useState } from "react";

import type { EditorSession } from "../engine/session";
import { statsLine, type FrameStats } from "./tools";

const REFRESH_MS = 1000;

export function RenderStatsReadout({ session }: { session: EditorSession }) {
  const [line, setLine] = useState<string>();
  useEffect(() => {
    let stopped = false;
    const refresh = () => {
      if (document.hidden) return;
      session.call<{ frame: FrameStats | null }>("render.stats").then(
        (r) => !stopped && setLine(r.frame === null ? undefined : statsLine(r.frame)),
        () => !stopped && setLine(undefined), // a project without rendering: no readout
      );
    };
    refresh();
    const timer = setInterval(refresh, REFRESH_MS);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [session]);
  return (
    <span role="status" aria-label="Render stats" style={{ fontSize: 11, opacity: 0.7, whiteSpace: "nowrap" }}>
      {line ?? ""}
    </span>
  );
}
