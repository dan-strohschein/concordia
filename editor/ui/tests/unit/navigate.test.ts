// Coalescing camera moves sent to `viewport.navigate` while one is in flight (M1 phase 1.9d).
import { mergeNav } from "../../src/viewport/navigate";

test("consecutive drags add up", () => {
  expect(mergeNav({ orbit: [1, 2] }, { orbit: [3, -1] })).toEqual({ orbit: [4, 1] });
  expect(mergeNav({ pan: [1, 0], height: 300 }, { pan: [2, 2], height: 300 })).toEqual({ pan: [3, 2], height: 300 });
  expect(mergeNav({ zoom: 1 }, { zoom: 1 })).toEqual({ zoom: 2 });
});

test("presets, focus and mixed moves are never merged", () => {
  expect(mergeNav({ preset: "top" }, { orbit: [1, 1] })).toBeUndefined();
  expect(mergeNav({ orbit: [1, 1] }, { focus: [0, 0, 0] })).toBeUndefined();
  expect(mergeNav({ orbit: [1, 1] }, { pan: [1, 1], height: 300 })).toBeUndefined();
});
