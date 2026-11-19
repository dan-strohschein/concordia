// The Studio panel (M3 phase 3.8, ADR-0083): the task board, pending approvals, Studio status and,
// in the desktop editor, the Claude Code launcher. Shown only in Studio projects.
import { useState } from "react";

import type { EditorSession } from "../engine/session";
import { Approvals } from "./Approvals";
import { Launcher } from "./Launcher";
import { tauriShell } from "./launcherShell";
import type { StudioState } from "./studio";
import { StudioStatus } from "./StudioStatus";
import { TaskBoard } from "./TaskBoard";

const TABS = ["Board", "Approvals", "Status", "Claude Code"] as const;
type Tab = (typeof TABS)[number];

export function StudioPanel({ session, studio }: { session: EditorSession; studio: StudioState }) {
  const [tab, setTab] = useState<Tab>("Board");
  if (!studio.teamMode) return null;
  const pending = studio.tasks.filter((t) => t.status === "in_review").length;
  const label = (t: Tab) => (t === "Approvals" && pending > 0 ? `Approvals (${pending})` : t);
  return (
    <section aria-label="Studio panel" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div role="tablist" aria-label="Studio" style={{ display: "flex", gap: 2 }}>
        {TABS.map((t) => (
          <button
            key={t}
            type="button"
            role="tab"
            id={`studio-tab-${t}`}
            aria-selected={tab === t}
            onClick={() => setTab(t)}
            style={{ background: tab === t ? "#3a3a3a" : "transparent", color: "inherit", border: "none" }}
          >
            {label(t)}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        aria-labelledby={`studio-tab-${tab}`}
        style={{ flex: 1, minHeight: 0, overflow: "auto", padding: 4 }}
      >
        {tab === "Board" && <TaskBoard session={session} studio={studio} />}
        {tab === "Approvals" && <Approvals session={session} studio={studio} />}
        {tab === "Status" && <StudioStatus session={session} studio={studio} />}
        {tab === "Claude Code" && (
          <Launcher shell={tauriShell()} budgetUsd={studio.status?.budgets.headless_max_budget_usd ?? null} />
        )}
      </div>
    </section>
  );
}
