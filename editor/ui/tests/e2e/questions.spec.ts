// Team questions end to end (M3 phase 3.6b, ADR-0081): an escalation from an agent shows the
// banner and badge live, the human answers in the panel, and the task unblocks.
import { expect, test } from "./engine";

test.use({ studio: true });

const live = { timeout: process.env["CI"] ? 15_000 : 1_000 };

test("an_escalation_shows_the_banner_and_the_human_answer_unblocks_the_task", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  await expect(page.getByRole("region", { name: "Agent Activity dock" })).toBeVisible();

  const { task } = await engine.agent.call<{ task: { id: string } }>("task.create", {
    title: "Arena floor",
    role: "level-designer",
    actor: "agent:director#1",
  });
  await engine.agent.call("task.claim", { id: task.id, actor: "agent:level-designer#1" });
  const { question } = await engine.agent.call<{ question: { id: string } }>("studio.ask", {
    to: "director",
    text: "arena size? 30 or 40 m?",
    task: task.id,
    actor: "agent:level-designer#1",
  });
  await engine.agent.call("studio.escalate", {
    id: question.id,
    reason: "arena scale is a design call",
    actor: "agent:director#1",
  });

  const banner = page.getByRole("alert", { name: "Escalated questions" });
  await expect(banner).toContainText(`The producer needs you: ${question.id}`, live);
  await expect(page.getByLabel("1 escalated question").first()).toBeVisible(live);

  const dock = page.getByRole("region", { name: "Team Questions dock" });
  const thread = dock.getByRole("listitem").filter({ hasText: question.id });
  await thread.getByRole("textbox", { name: `Answer ${question.id}` }).fill("40 m square");
  await thread.getByRole("button", { name: "Answer" }).click();
  await expect(banner).toHaveCount(0);

  const { questions } = await engine.agent.call<{
    questions: { id: string; answer?: { by: string; text: string } }[];
  }>("studio.questions", {});
  expect(questions.find((q) => q.id === question.id)?.answer).toMatchObject({ by: "human:local", text: "40 m square" });
  const after = await engine.agent.call<{ task: { status: string } }>("task.get", { id: task.id });
  expect(after.task.status).toBe("todo");
});
