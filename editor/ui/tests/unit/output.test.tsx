// The Output panel (M1 phase 1.9e): fetches log records when the engine announces them, and does
// nothing while the editor is idle.
import { act, render, screen } from "@testing-library/react";
import { vi } from "vitest";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { Output } from "../../src/output/Output";
import { FakeEngine, oneEntity } from "./support/engine";

test("an idle editor never polls the log; announced records are fetched", async () => {
  let records = [{ seq: 1, level: "INFO", target: "t", message: "hello", fields: {} }];
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "log.tail": () => ({ records, next: records.at(-1)?.seq ?? 0 }),
  });
  const session = await EditorSession.open(new EngineClient(engine));
  vi.useFakeTimers();
  try {
    render(<Output session={session} />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(engine.called("log.tail")).toHaveLength(1);
    records = [{ seq: 2, level: "INFO", target: "t", message: "agent edit", fields: {} }];
    await act(async () => {
      engine.event({ topic: "log", latest: 2 });
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(engine.called("log.tail")).toHaveLength(2);
  } finally {
    vi.useRealTimers();
  }
  expect(await screen.findByText(/agent edit/)).toBeTruthy();
});
