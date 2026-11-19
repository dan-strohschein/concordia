import { describe, expect, it } from "vitest";

import { percentile, summarize, type FrameRecord } from "../../src/bench/stats";
import { InputTags } from "../../src/viewport/latency";

const frame = (seq: number, input: number, submitted: number, presented: number | null): FrameRecord => ({
  seq,
  input_time_ms: input,
  cpu_ms: 2,
  submitted_ms: submitted,
  presented_ms: presented,
});

describe("percentile", () => {
  it("uses nearest rank and rejects empty or out-of-range input", () => {
    expect(percentile([5, 1, 3, 2, 4], 50)).toBe(3);
    expect(percentile([5, 1, 3, 2, 4], 100)).toBe(5);
    expect(percentile([7], 95)).toBe(7);
    expect(percentile([], 50)).toBeNaN();
    expect(() => percentile([1], 101)).toThrow(RangeError);
  });
});

describe("summarize", () => {
  it("measures input-to-photon from each input's first presentation", () => {
    const frames = [frame(1, 0, 4, 20), frame(1, 0, 20, 36), frame(2, 10, 36, 52), frame(3, 30, 52, null)];
    const s = summarize(frames, 16);
    expect(s.latency_ms.p50).toBe(20); // seq 1: 20 - 0; seq 2: 52 - 10 = 42
    expect(s.latency_ms.max).toBe(42);
    expect(s.inputs).toBe(2); // seq 3 never reached the display
    expect(s.unpresented).toBe(1);
  });

  it("measures pacing between presented frames and counts missed refreshes", () => {
    const presented = [0, 16, 32, 64, 80, 128];
    const s = summarize(
      presented.map((p, i) => frame(i, p - 5, p - 4, p)),
      16,
    );
    expect(s.frames).toBe(6);
    expect(s.interval_ms.p50).toBe(16);
    expect(s.interval_ms.max).toBe(48);
    expect(s.missed_refreshes).toBe(1 + 2); // 32 → 64 skips one, 80 → 128 skips two
    expect(s.fps).toBeCloseTo((5 / 128) * 1000, 5);
  });

  it("rejects a non-positive refresh interval", () => {
    expect(() => summarize([], 0)).toThrow(RangeError);
  });
});

describe("InputTags", () => {
  it("numbers inputs and stamps them on the page's Unix-ms clock", () => {
    const tags = new InputTags(() => 1_000);
    expect(tags.current()).toBeUndefined();
    expect(tags.mark(5)).toEqual({ seq: 1, input_time_ms: 1_005 });
    expect(tags.mark(9)).toEqual({ seq: 2, input_time_ms: 1_009 });
    expect(tags.current()).toEqual({ seq: 2, input_time_ms: 1_009 });
  });
});
