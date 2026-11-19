import { describe, expect, it } from "vitest";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { FakeEngine, oneEntity } from "./support/engine";

describe("EditorSession.entities", () => {
  it("follows query cursors so large scenes are listed in full", async () => {
    const pages: Record<string, { rows: { id: string; components: Record<string, unknown> }[]; next_cursor?: string }> =
      {
        start: { rows: [{ id: "e1", components: { Name: "A" } }], next_cursor: "p2" },
        p2: { rows: [{ id: "e2", components: { Name: "B" } }], next_cursor: "p3" },
        p3: { rows: [{ id: "e3", components: { Name: "C" } }] },
      };
    const engine = new FakeEngine({
      ...oneEntity({ translation: [0, 0, 0] }),
      "query.run": (p) => pages[typeof p["cursor"] === "string" ? p["cursor"] : "start"],
    });
    const session = await EditorSession.open(new EngineClient(engine));
    expect((await session.entities()).map((r) => r.id)).toEqual(["e1", "e2", "e3"]);
    expect(engine.called("query.run").map((p) => p["cursor"])).toEqual([undefined, "p2", "p3"]);
  });
});
