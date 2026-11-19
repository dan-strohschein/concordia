// Studio status (ADR-0083): instances in use, budget limits, the last sync, the doctor's checks
// and "Sync now". Budget usage is counted by `concordia mcp` and not shown yet.
import { useState } from "react";

import type { EditorSession } from "../engine/session";
import { type DoctorCheck, runDoctor, type StudioState, syncNow } from "./studio";

const time = (ms: number) => new Date(ms).toLocaleString();
const MARK: Record<DoctorCheck["status"], string> = { ok: "✓", note: "•", fail: "✗" };

export function StudioStatus({ session, studio }: { session: EditorSession; studio: StudioState }) {
  const [checks, setChecks] = useState<DoctorCheck[]>();
  const [synced, setSynced] = useState<string>();
  const [error, setError] = useState<string>();
  const fail = (e: unknown) => setError(e instanceof Error ? e.message : String(e));
  const status = studio.status;
  const sync = () =>
    syncNow(session).then((r) => {
      const changed = r.changes.filter((c) => c.action !== "unchanged").length;
      const refused = r.refused.length > 0 ? `; hand-edited, left alone: ${r.refused.join(", ")}` : "";
      setSynced(`${changed} file${changed === 1 ? "" : "s"} changed${refused}`);
      studio.reload();
    }, fail);
  return (
    <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: 8 }}>
      <section aria-label="Instances">
        <strong>Instances</strong>
        <ul style={{ margin: 0, paddingLeft: 16 }}>
          {status?.instances.map((i) => (
            <li key={`${i.role}#${i.slot}`}>
              agent:{i.role}#{i.slot} <span style={{ opacity: 0.5 }}>{i.agent}</span>
            </li>
          ))}
          {status?.instances.length === 0 && <li style={{ opacity: 0.6 }}>no agents running</li>}
        </ul>
      </section>
      <section aria-label="Budgets">
        <strong>Budget limits</strong>
        <ul style={{ margin: 0, paddingLeft: 16 }}>
          <li>turns per subagent: {status?.budgets.max_turns_per_subagent ?? "none"}</li>
          <li>engine calls per task: {status?.budgets.max_mcp_calls_per_task ?? "none"}</li>
          <li>mutations per task: {status?.budgets.max_mutations_per_task ?? "none"}</li>
          <li>headless run cap (USD): {status?.budgets.headless_max_budget_usd ?? "none"}</li>
        </ul>
      </section>
      <section aria-label="Sync">
        <strong>Claude Code files</strong>
        <div>
          {status?.last_sync
            ? `last synced ${time(status.last_sync.ts_ms)} by ${status.last_sync.actor} (${status.last_sync.changed} files)`
            : "never synced from here"}
        </div>
        <button type="button" onClick={() => void sync()}>
          Sync now
        </button>{" "}
        {synced}
      </section>
      <section aria-label="Doctor">
        <strong>Doctor</strong>{" "}
        <button type="button" onClick={() => void runDoctor(session).then(setChecks, fail)}>
          Run checks
        </button>
        {checks && (
          <ul aria-label="Doctor checks" style={{ margin: 0, paddingLeft: 16 }}>
            {checks.map((c) => (
              <li key={c.id} style={{ color: c.status === "fail" ? "#f88" : undefined }}>
                {MARK[c.status]} {c.text}
              </li>
            ))}
          </ul>
        )}
      </section>
      {error && (
        <p role="alert" style={{ color: "#f88" }}>
          {error}
        </p>
      )}
    </div>
  );
}
