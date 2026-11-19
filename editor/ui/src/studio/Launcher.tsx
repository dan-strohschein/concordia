// The Claude Code launcher tab (ADR-0083): the user's own `claude` and how it is logged in (shown,
// never stored), an interactive Director in a terminal, and a headless run whose stream is
// grouped by subagent. Desktop editor only.
import { useEffect, useState } from "react";

import { type Detection, groupEvents, type LauncherShell, type StreamEvent } from "./launcherShell";

/** Where the chosen terminal app is remembered (a per-viewer convenience). */
const TERMINAL_KEY = "concordia.launcher.terminal";

function savedTerminal(): string {
  try {
    return localStorage.getItem(TERMINAL_KEY) ?? "Terminal";
  } catch {
    return "Terminal";
  }
}

function saveTerminal(app: string): void {
  try {
    localStorage.setItem(TERMINAL_KEY, app);
  } catch {
    // Storage is optional.
  }
}

interface Props {
  shell: LauncherShell | undefined;
  /** `[budgets] headless_max_budget_usd` from studio/team.toml. */
  budgetUsd: number | null;
}

export function Launcher({ shell, budgetUsd }: Props) {
  if (shell === undefined) {
    return <p role="note">The Claude Code launcher is available in the desktop editor.</p>;
  }
  return <ShellLauncher shell={shell} budgetUsd={budgetUsd} />;
}

function ShellLauncher({ shell, budgetUsd }: { shell: LauncherShell; budgetUsd: number | null }) {
  const [found, setFound] = useState<Detection>();
  const [error, setError] = useState<string>();
  const [app, setApp] = useState(savedTerminal);
  const [prompt, setPrompt] = useState("");
  const [events, setEvents] = useState<StreamEvent[]>([]);
  const [running, setRunning] = useState(false);
  const [ended, setEnded] = useState<string>();
  const fail = (e: unknown) => setError(e instanceof Error ? e.message : String(e));
  useEffect(() => {
    shell.detect().then(setFound, (e: unknown) => setError(String(e)));
    return shell.listen(
      (e) => setEvents((list) => [...list, e]),
      (code, err) => {
        setRunning(false);
        setEnded(err ?? `claude exited with ${code ?? "a signal"}`);
      },
    );
  }, [shell]);
  const open = () => {
    saveTerminal(app);
    shell.openTerminal(app).catch(fail);
  };
  const run = () => {
    setEvents([]);
    setEnded(undefined);
    setRunning(true);
    shell.run(prompt, budgetUsd).catch((e: unknown) => {
      setRunning(false);
      fail(e);
    });
  };
  const installed = found?.claude != null;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {found && <Login found={found} />}
      {error && (
        <p role="alert" style={{ color: "#f88", margin: 0 }}>
          {error}
        </p>
      )}
      <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
        <label>
          Terminal app <input aria-label="Terminal app" value={app} onChange={(e) => setApp(e.target.value)} />
        </label>
        <button type="button" disabled={app.trim() === ""} onClick={open}>
          Open Director in {app.trim() || "a terminal"}
        </button>
      </div>
      <div style={{ display: "flex", gap: 4, alignItems: "flex-start" }}>
        <textarea
          aria-label="Prompt"
          rows={2}
          placeholder="What should the Director do? e.g. Work the board until T-0001 is done."
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          style={{ flex: 1 }}
        />
        <button type="button" disabled={!installed || running || prompt.trim() === ""} onClick={run}>
          Run headless
        </button>
        <button type="button" disabled={!running} onClick={() => void shell.stop().catch(fail)}>
          Stop
        </button>
      </div>
      {budgetUsd !== null && <div style={{ opacity: 0.6 }}>cap: --max-budget-usd {budgetUsd} (an estimate)</div>}
      <Stream events={events} />
      {ended && <div style={{ opacity: 0.7 }}>{ended}</div>}
    </div>
  );
}

function Login({ found }: { found: Detection }) {
  if (found.claude === null) {
    return (
      <p role="alert" style={{ color: "#f88", margin: 0 }}>
        {found.install_hint ?? "Claude Code is not installed"}
      </p>
    );
  }
  return (
    <div>
      Claude Code {found.version ?? "(version unknown)"} ·{" "}
      <span>
        logged in with {found.auth_method ?? "an unknown method"}
        {found.subscription ? " (a subscription; no API key needed)" : ""}
      </span>
    </div>
  );
}

function Stream({ events }: { events: readonly StreamEvent[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {groupEvents(events).map(({ group, events: list }) => (
        <fieldset key={group ?? "director"} role="group" aria-label={group === null ? "Director" : `Subagent ${group}`}>
          <legend>{group === null ? "Director" : `Subagent ${group}`}</legend>
          <ul style={{ margin: 0, paddingLeft: 16 }}>
            {list.map((e, i) => (
              <li key={i} style={{ opacity: e.kind === "user" ? 0.6 : 1 }}>
                {e.text}
              </li>
            ))}
          </ul>
        </fieldset>
      ))}
    </div>
  );
}
