// The Claude Code launcher tab (M3 phase 3.8, ADR-0083): login method or install hint, an
// interactive Director in a terminal, and a headless run whose stream is grouped by subagent.
import { act, fireEvent, render, screen, within } from "@testing-library/react";

import { Launcher } from "../../src/studio/Launcher";
import { type LauncherShell, type StreamEvent, parseDetection, parseStreamEvent } from "../../src/studio/launcherShell";

/** A fake shell that records calls and lets the test push stream events. */
function fakeShell(detection: unknown) {
  const calls: { command: string; args: unknown }[] = [];
  let emit: (e: StreamEvent) => void = () => {};
  let done: (code: number | null, error: string | null) => void = () => {};
  const shell: LauncherShell = {
    detect: () => Promise.resolve(parseDetection(detection)),
    openTerminal: (app) => {
      calls.push({ command: "open", args: app });
      return Promise.resolve();
    },
    run: (prompt, budget) => {
      calls.push({ command: "run", args: { prompt, budget } });
      return Promise.resolve();
    },
    stop: () => {
      calls.push({ command: "stop", args: null });
      return Promise.resolve(true);
    },
    listen: (onEvent, onDone) => {
      emit = onEvent;
      done = onDone;
      return () => {};
    },
  };
  return { shell, calls, push: (e: StreamEvent) => emit(e), finish: (code: number) => done(code, null) };
}

const SUBSCRIBED = {
  claude: "/opt/bin/claude",
  version: "2.1.328 (Claude Code)",
  auth_method: "claude.ai",
  subscription: true,
  install_hint: null,
};

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

test("launcher_is_hidden_outside_the_desktop_app", async () => {
  render(<Launcher shell={undefined} budgetUsd={null} />);
  await settle();
  expect(screen.getByRole("note").textContent).toContain("desktop editor");
  expect(screen.queryByRole("button", { name: /Open Director/ })).toBeNull();
});

test("launcher_shows_the_login_method_or_the_install_hint", async () => {
  const ok = fakeShell(SUBSCRIBED);
  const { unmount } = render(<Launcher shell={ok.shell} budgetUsd={null} />);
  await settle();
  expect(screen.getByText(/2\.1\.328/)).toBeTruthy();
  expect(screen.getByText(/claude\.ai/).textContent).toContain("subscription");
  unmount();
  const missing = fakeShell({
    ...SUBSCRIBED,
    claude: null,
    version: null,
    auth_method: null,
    subscription: false,
    install_hint: "install it from https://code.claude.com",
  });
  render(<Launcher shell={missing.shell} budgetUsd={null} />);
  await settle();
  expect(screen.getByRole("alert").textContent).toContain("code.claude.com");
  expect((screen.getByRole("button", { name: "Run headless" }) as HTMLButtonElement).disabled).toBe(true);
});

test("open_terminal_and_run_call_the_shell_with_the_team_budget", async () => {
  const fake = fakeShell(SUBSCRIBED);
  render(<Launcher shell={fake.shell} budgetUsd={2.5} />);
  await settle();
  fireEvent.change(screen.getByRole("textbox", { name: "Terminal app" }), { target: { value: "iTerm" } });
  fireEvent.click(screen.getByRole("button", { name: "Open Director in iTerm" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Prompt" }), { target: { value: "Work the board." } });
  fireEvent.click(screen.getByRole("button", { name: "Run headless" }));
  await settle();
  expect(fake.calls).toEqual([
    { command: "open", args: "iTerm" },
    { command: "run", args: { prompt: "Work the board.", budget: 2.5 } },
  ]);
  fireEvent.click(screen.getByRole("button", { name: "Stop" }));
  await settle();
  expect(fake.calls.at(-1)?.command).toBe("stop");
});

test("headless_stream_is_grouped_by_subagent", async () => {
  const fake = fakeShell(SUBSCRIBED);
  render(<Launcher shell={fake.shell} budgetUsd={null} />);
  await settle();
  fireEvent.change(screen.getByRole("textbox", { name: "Prompt" }), { target: { value: "go" } });
  fireEvent.click(screen.getByRole("button", { name: "Run headless" }));
  await settle();
  await act(async () => {
    fake.push({ group: null, kind: "assistant", text: "Planning · Agent" });
    fake.push({ group: "toolu_1", kind: "assistant", text: "mcp__concordia__task_claim" });
    fake.push({ group: null, kind: "assistant", text: "Routing to QA" });
    fake.push({ group: "toolu_1", kind: "user", text: "tool result" });
    fake.finish(0);
    await Promise.resolve();
  });
  const groups = screen.getAllByRole("group");
  expect(groups.map((g) => g.getAttribute("aria-label"))).toEqual(["Director", "Subagent toolu_1"]);
  expect(
    within(groups[0] as HTMLElement)
      .getAllByRole("listitem")
      .map((i) => i.textContent),
  ).toEqual(["Planning · Agent", "Routing to QA"]);
  expect(within(groups[1] as HTMLElement).getAllByRole("listitem")).toHaveLength(2);
  expect(screen.getByText(/exited with 0/)).toBeTruthy();
});

test("ipc_payloads_are_validated", () => {
  expect(parseStreamEvent({ group: null, kind: "result", text: "success" })).toEqual({
    group: null,
    kind: "result",
    text: "success",
  });
  expect(parseStreamEvent({ kind: 3 })).toBeUndefined();
  expect(() => parseDetection({ claude: 1 })).toThrow();
});
