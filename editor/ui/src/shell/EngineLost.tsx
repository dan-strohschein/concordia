// A banner shown when the shared engine's host exits (ADR-0082): the editor can't keep working
// on an engine that is gone, so it says so instead of failing silently.
import { useEffect, useState } from "react";

import type { EngineClient } from "../api/client";

const banner: React.CSSProperties = {
  position: "fixed",
  top: 0,
  left: 0,
  right: 0,
  zIndex: 1000,
  padding: "8px 16px",
  background: "#7a1f1f",
  color: "#fff",
  font: "13px system-ui",
  textAlign: "center",
};

export function EngineLost({ client }: { client: EngineClient }) {
  const [lost, setLost] = useState(client.lost);
  useEffect(() => client.onLost(() => setLost(true)), [client]);
  if (!lost) return null;
  return (
    <div role="alert" style={banner}>
      The engine has exited (the process hosting it closed). Your saved work is on disk; restart the editor to
      reconnect.
    </div>
  );
}
