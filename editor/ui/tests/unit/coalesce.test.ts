// Refreshes coalesced while one is in flight (M1 phase 1.9e): a drag's stream of change events
// costs at most one fetch in flight plus one trailing fetch.
import { Coalescer } from "../../src/engine/coalesce";

test("requests during a run collapse into one trailing run", async () => {
  let runs = 0;
  let release: () => void = () => undefined;
  const c = new Coalescer(
    () =>
      new Promise<void>((resolve) => {
        runs += 1;
        release = resolve;
      }),
  );
  for (let i = 0; i < 50; i++) c.request();
  expect(runs).toBe(1);
  release();
  await new Promise((r) => setTimeout(r, 0));
  expect(runs).toBe(2);
  release();
  await new Promise((r) => setTimeout(r, 0));
  expect(runs).toBe(2);
  c.request();
  expect(runs).toBe(3);
});

test("a failed run does not stop later ones", async () => {
  let runs = 0;
  const c = new Coalescer(() => {
    runs += 1;
    return Promise.reject(new Error("offline"));
  });
  c.request();
  await new Promise((r) => setTimeout(r, 0));
  c.request();
  expect(runs).toBe(2);
});
