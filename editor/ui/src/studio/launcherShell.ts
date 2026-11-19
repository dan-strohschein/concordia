// The Claude Code launcher's shell commands (ADR-0083): only the desktop editor has them. The
// payloads cross the IPC boundary, so they are checked before use.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** What the shell found about Claude Code (`launcher_detect`). */
export interface Detection {
  claude: string | null;
  version: string | null;
  auth_method: string | null;
  subscription: boolean;
  install_hint: string | null;
}

/** One summarized stream-json line (`launcher://event`). */
export interface StreamEvent {
  /** The subagent's parent tool use; null for the Director. */
  group: string | null;
  kind: string;
  text: string;
}

/** The desktop shell's launcher commands. */
export interface LauncherShell {
  detect: () => Promise<Detection>;
  openTerminal: (app: string) => Promise<void>;
  run: (prompt: string, budgetUsd: number | null) => Promise<void>;
  stop: () => Promise<boolean>;
  /** Follows the headless stream; returns the unsubscribe function. */
  listen: (
    onEvent: (e: StreamEvent) => void,
    onDone: (code: number | null, error: string | null) => void,
  ) => () => void;
}

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;
const optString = (v: unknown): v is string | null => v === null || typeof v === "string";

/** Checks a `launcher_detect` reply. */
export function parseDetection(value: unknown): Detection {
  if (
    isRecord(value) &&
    optString(value["claude"]) &&
    optString(value["version"]) &&
    optString(value["auth_method"]) &&
    typeof value["subscription"] === "boolean" &&
    optString(value["install_hint"])
  ) {
    return {
      claude: value["claude"],
      version: value["version"],
      auth_method: value["auth_method"],
      subscription: value["subscription"],
      install_hint: value["install_hint"],
    };
  }
  throw new TypeError("the shell's launcher_detect reply has an unexpected shape");
}

/** Checks a `launcher://event` payload; undefined if it isn't one. */
export function parseStreamEvent(value: unknown): StreamEvent | undefined {
  if (!isRecord(value)) return undefined;
  const { group, kind, text } = value;
  if (!optString(group) || typeof kind !== "string" || typeof text !== "string") return undefined;
  return { group, kind, text };
}

function parseDone(value: unknown): { code: number | null; error: string | null } {
  const v = isRecord(value) ? value : {};
  return {
    code: typeof v["code"] === "number" ? v["code"] : null,
    error: typeof v["error"] === "string" ? v["error"] : null,
  };
}

/** The desktop shell's launcher, or undefined in a browser. */
export function tauriShell(): LauncherShell | undefined {
  if (!isTauri()) return undefined;
  return {
    detect: async () => parseDetection(await invoke("launcher_detect")),
    openTerminal: (app) => invoke("launcher_open_terminal", { app }),
    run: (prompt, budgetUsd) => invoke("launcher_run", { prompt, maxBudgetUsd: budgetUsd }),
    stop: async () => (await invoke("launcher_stop")) === true,
    listen: (onEvent, onDone) => {
      const stops = [
        listen("launcher://event", (e) => {
          const event = parseStreamEvent(e.payload);
          if (event !== undefined) onEvent(event);
        }),
        listen("launcher://done", (e) => {
          const done = parseDone(e.payload);
          onDone(done.code, done.error);
        }),
      ];
      return () => stops.forEach((p) => void p.then((stop) => stop()));
    },
  };
}

/** Events grouped by subagent, Director first, each group in arrival order. */
export function groupEvents(events: readonly StreamEvent[]): { group: string | null; events: StreamEvent[] }[] {
  const groups = new Map<string | null, StreamEvent[]>([[null, []]]);
  for (const e of events) groups.set(e.group, [...(groups.get(e.group) ?? []), e]);
  return [...groups].filter(([, list]) => list.length > 0).map(([group, list]) => ({ group, events: list }));
}
