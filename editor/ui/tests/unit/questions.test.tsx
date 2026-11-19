// The Team Questions panel (M3 phase 3.6b, ADR-0081): threads, pinned escalations with a badge, a
// banner until every escalation is answered, and human answers through studio.answer.
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { EscalationBadge, TeamDesk } from "../../src/questions/TeamQuestions";
import { type Question, useTeamQuestions } from "../../src/questions/questions";
import { FakeEngine } from "./support/engine";

const T0 = 1_763_500_000_000;

const q = (id: string, status: Question["status"], extra: Partial<Question> = {}): Question => ({
  id,
  from: "agent:level-designer#1",
  to: status === "escalated" ? "human" : "tech-artist",
  task: "T-0001",
  refs: [],
  text: `question ${id}?`,
  asked_ms: T0,
  status,
  ...(status === "escalated" ? { escalation: { by: "agent:director#1", reason: "design call", ms: T0 + 1000 } } : {}),
  ...extra,
});

/** A fake Studio engine whose question list the test edits. */
function studio(questions: Question[], teamMode = true) {
  const state = { questions };
  const engine = new FakeEngine({
    "session.open": () => ({ session: "s1", concordia_version: "0", protocol: "1" }),
    "events.subscribe": (p) => ({ topics: p["topics"] }),
    ...(teamMode
      ? {
          "studio.questions": () => ({ questions: state.questions, next: 1 }),
          "studio.answer": (p) => {
            const id = String(p["id"]);
            state.questions = state.questions.map((x) =>
              x.id === id
                ? {
                    ...x,
                    status: "answered" as const,
                    answer: { by: String(p["actor"]), text: String(p["text"]), ms: T0 + 5000 },
                  }
                : x,
            );
            return { question: state.questions.find((x) => x.id === id) };
          },
        }
      : {}),
  });
  return { engine, state };
}

function Harness({ session, initiallyShown }: { session: EditorSession; initiallyShown: boolean }) {
  const team = useTeamQuestions(session);
  const [shown, setShown] = useState(initiallyShown);
  return (
    <>
      <header>
        <EscalationBadge team={team} />
      </header>
      <TeamDesk session={session} team={team} shown={shown} onReveal={() => setShown(true)} />
    </>
  );
}

async function mount(engine: FakeEngine, initiallyShown = true) {
  const session = await EditorSession.open(new EngineClient(engine));
  render(<Harness session={session} initiallyShown={initiallyShown} />);
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

async function push(engine: FakeEngine, kind: string) {
  await act(async () => {
    engine.event({ topic: "studio", kind });
    await new Promise((r) => setTimeout(r, 0));
  });
}

test("questions_panel_shows_threads_with_actors_and_times", async () => {
  const answered = q("Q-0001", "answered", {
    answer: { by: "agent:tech-artist#1", text: "stone floor", ms: T0 + 60_000 },
  });
  const { engine } = studio([answered]);
  await mount(engine);
  const thread = within(screen.getByRole("region", { name: "Team Questions dock" })).getByRole("listitem");
  expect(thread.textContent).toContain("agent:level-designer#1 → tech-artist");
  expect(thread.textContent).toContain("T-0001");
  expect(thread.textContent).toContain("question Q-0001?");
  expect(thread.textContent).toContain(new Date(T0).toLocaleTimeString());
  expect(thread.textContent).toContain("agent:tech-artist#1");
  expect(thread.textContent).toContain("stone floor");
  expect(thread.textContent).toContain(new Date(T0 + 60_000).toLocaleTimeString());
});

test("escalations_are_pinned_and_counted_on_the_badge", async () => {
  const { engine } = studio([q("Q-0001", "open"), q("Q-0002", "escalated"), q("Q-0003", "escalated")]);
  await mount(engine);
  const ids = within(screen.getByRole("region", { name: "Team Questions dock" }))
    .getAllByRole("listitem")
    .map((li) => li.getAttribute("data-question"));
  expect(ids).toEqual(["Q-0002", "Q-0003", "Q-0001"]);
  expect(screen.getAllByLabelText("2 escalated questions").length).toBeGreaterThanOrEqual(2);
});

test("a_new_escalation_reveals_the_panel_and_shows_the_banner", async () => {
  const { engine, state } = studio([q("Q-0001", "open")]);
  await mount(engine, false);
  expect(screen.queryByRole("region", { name: "Team Questions dock" })).toBeNull();
  expect(screen.queryByRole("alert", { name: "Escalated questions" })).toBeNull();
  state.questions = [q("Q-0001", "escalated")];
  await push(engine, "question_escalated");
  expect(screen.getByRole("region", { name: "Team Questions dock" })).toBeTruthy();
  const banner = screen.getByRole("alert", { name: "Escalated questions" });
  expect(banner.textContent).toContain("The producer needs you: Q-0001");
});

test("the_banner_stays_until_every_escalation_is_answered", async () => {
  const { engine, state } = studio([q("Q-0001", "escalated"), q("Q-0002", "escalated")]);
  await mount(engine);
  const banner = () => screen.queryByRole("alert", { name: "Escalated questions" });
  expect(banner()).toBeTruthy();
  state.questions = [
    q("Q-0001", "answered", { answer: { by: "human:local", text: "a", ms: T0 } }),
    q("Q-0002", "escalated"),
  ];
  await push(engine, "question_answered");
  expect(banner()).toBeTruthy();
  expect(banner()?.textContent).toContain("Q-0002");
  state.questions = state.questions.map((x) => ({ ...x, status: "answered" as const }));
  await push(engine, "question_answered");
  expect(banner()).toBeNull();
});

test("answering_calls_studio_answer_as_the_local_human", async () => {
  const { engine } = studio([q("Q-0002", "escalated")]);
  await mount(engine);
  const thread = screen.getByRole("listitem");
  fireEvent.change(within(thread).getByRole("textbox", { name: "Answer Q-0002" }), {
    target: { value: "40 m square" },
  });
  await act(async () => {
    fireEvent.click(within(thread).getByRole("button", { name: "Answer" }));
    await new Promise((r) => setTimeout(r, 0));
  });
  expect(engine.called("studio.answer")).toEqual([{ id: "Q-0002", text: "40 m square", actor: "human:local" }]);
  expect(screen.queryByRole("alert", { name: "Escalated questions" })).toBeNull();
});

test("the_answer_box_enforces_the_length_cap", async () => {
  const { engine } = studio([q("Q-0002", "escalated")]);
  await mount(engine);
  const box = screen.getByRole("textbox", { name: "Answer Q-0002" }) as HTMLTextAreaElement;
  expect(box.maxLength).toBe(500);
  fireEvent.change(box, { target: { value: "x".repeat(510) } });
  expect(box.value.length).toBe(500);
  expect(screen.getByText("500/500")).toBeTruthy();
});

test("the_panel_is_hidden_outside_team_mode", async () => {
  const { engine } = studio([], false);
  await mount(engine);
  expect(screen.queryByRole("region", { name: "Team Questions dock" })).toBeNull();
  expect(screen.queryByRole("alert", { name: "Escalated questions" })).toBeNull();
  expect(screen.queryByLabelText(/escalated questions/)).toBeNull();
});
