// The viewport's frame queue (M1 1.12 fix): one capture in flight, the newest request queued, and each
// request renders the world it was made for, even when it runs after the world changed.
import { FrameQueue } from "../../src/viewport/frameQueue";

function deferred() {
  let resolve: () => void = () => undefined;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}

test("a request queued behind an in-flight frame renders its own world, newest wins", async () => {
  const rendered: string[] = [];
  const gates: (() => void)[] = [];
  const queue = new FrameQueue<{ world: string }>((job) => {
    rendered.push(job.world);
    const d = deferred();
    gates.push(d.resolve);
    return d.promise;
  });
  queue.request({ world: "edit" }); // starts at once
  queue.request({ world: "edit" }); // replaced by the next
  queue.request({ world: "play" }); // queued
  expect(rendered).toEqual(["edit"]);
  gates.shift()?.();
  await Promise.resolve();
  await Promise.resolve();
  expect(rendered).toEqual(["edit", "play"]);
  gates.shift()?.();
});

test("a failed frame does not stall the queue", async () => {
  const rendered: number[] = [];
  const queue = new FrameQueue<{ n: number }>((job) => {
    rendered.push(job.n);
    return job.n === 1 ? Promise.reject(new Error("boom")) : Promise.resolve();
  });
  queue.request({ n: 1 });
  queue.request({ n: 2 });
  await new Promise((r) => setTimeout(r, 0));
  expect(rendered).toEqual([1, 2]);
});
