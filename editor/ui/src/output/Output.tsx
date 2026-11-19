// The bottom Output panel: polls `log.tail` and shows level, actor and message.
import { useEffect, useState } from "react";

import type { EditorSession } from "../engine/session";

interface LogRecord {
  seq: number;
  level: string;
  message: string;
  fields?: Record<string, unknown>;
}

const MAX_LINES = 500;

export function Output({ session }: { session: EditorSession }) {
  const [lines, setLines] = useState<LogRecord[]>([]);
  useEffect(() => {
    let since: number | undefined;
    let stopped = false;
    const poll = async () => {
      try {
        const page = await session.call<{ records: LogRecord[]; next: number }>(
          "log.tail",
          since === undefined ? {} : { since },
        );
        since = page.next;
        if (!stopped && page.records.length > 0) setLines((old) => [...old, ...page.records].slice(-MAX_LINES));
      } catch {
        // the connection reports its own errors; keep polling
      }
    };
    void poll();
    const timer = setInterval(poll, 1000);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [session]);
  return (
    <div
      role="log"
      aria-label="Output"
      style={{ fontFamily: "monospace", fontSize: 12, overflow: "auto", height: "100%", padding: 4 }}
    >
      {lines.map((l) => (
        <div key={l.seq}>
          <span style={{ opacity: 0.6 }}>{l.level}</span> {String(l.fields?.["actor"] ?? "")} {l.message}
        </div>
      ))}
    </div>
  );
}
