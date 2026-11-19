// The editor says so when the shared engine's host exits (M3 phase 3.5b, ADR-0082).
import { act, render, screen } from "@testing-library/react";

import { EngineClient, type Transport } from "../../src/api/client";
import { EngineLost } from "../../src/shell/EngineLost";

class DroppableTransport implements Transport {
  private dropped: (() => void) | undefined;
  send(): void {}
  onMessage(): void {}
  close(): void {}
  onClose(handler: () => void): void {
    this.dropped = handler;
  }
  drop(): void {
    this.dropped?.();
  }
}

test("the_editor_shows_a_banner_when_the_engine_exits", () => {
  const transport = new DroppableTransport();
  const client = new EngineClient(transport);
  render(<EngineLost client={client} />);
  expect(screen.queryByRole("alert")).toBeNull();
  act(() => transport.drop());
  const alert = screen.getByRole("alert");
  expect(alert.textContent).toMatch(/engine.*exited/i);
  expect(alert.textContent).toMatch(/restart the editor/i);
});
