// The shell's native flags (M4 4.1v(c)): the macOS shell grabs the gizmo with its own input;
// in a browser and in the Linux shell the page does.
import { afterEach, describe, expect, it } from "vitest";
import { isNative, pageGrabsGizmo } from "../../src/viewport/native";

afterEach(() => {
  delete window.__CONCORDIA_NATIVE__;
  delete window.__CONCORDIA_NATIVE_INPUT__;
});

describe("native flags", () => {
  it("a browser page grabs the gizmo itself", () => {
    expect(isNative()).toBe(false);
    expect(pageGrabsGizmo()).toBe(true);
  });

  it("the macOS shell's native input grabs it", () => {
    window.__CONCORDIA_NATIVE__ = true;
    window.__CONCORDIA_NATIVE_INPUT__ = true;
    expect(pageGrabsGizmo()).toBe(false);
  });

  it("a native viewport without native input (Linux) leaves it to the page", () => {
    window.__CONCORDIA_NATIVE__ = true;
    window.__CONCORDIA_NATIVE_INPUT__ = false;
    expect(isNative()).toBe(true);
    expect(pageGrabsGizmo()).toBe(true);
  });
});

// The window the shell places and the frames the page presents use one size (M4 4.1v(c)): an X11
// Vulkan surface must match its window exactly.
describe("the slot's size", () => {
  it("is whole CSS pixels, at least 16", async () => {
    const { slotSize } = await import("../../src/viewport/native");
    expect(slotSize({ width: 859.6, height: 701.4 })).toEqual({ width: 860, height: 701 });
    expect(slotSize({ width: 3, height: 0 })).toEqual({ width: 16, height: 16 });
  });

  it("is what the slot report says, beside its position, scale and the page height", async () => {
    const { slotRect } = await import("../../src/viewport/native");
    const rect = slotRect({ left: 10.5, top: 40.25, width: 859.6, height: 701.4 }, 2, 900);
    expect(rect).toEqual({ x: 10.5, y: 40.25, width: 860, height: 701, scale: 2, viewport_height: 900 });
  });
});
