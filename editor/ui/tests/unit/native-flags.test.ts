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
