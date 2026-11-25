// Native-shell mode (ADR-0043 as amended): the native view (Metal on macOS, an X11 child window on
// Vulkan on Linux) sits at the viewport slot. The page reports the slot to Rust and asks the engine
// to present frames.

declare global {
  interface Window {
    /** Set by the native shell. */
    __CONCORDIA_NATIVE__?: boolean;
    /** Set by a shell whose own input grabs the gizmo and drives the camera (macOS). */
    __CONCORDIA_NATIVE_INPUT__?: boolean;
  }
}

/** True inside the native editor shell. */
export const isNative = (): boolean => typeof window !== "undefined" && window.__CONCORDIA_NATIVE__ === true;

/** True unless the shell's native input grabs the engine-drawn gizmo. */
export const pageGrabsGizmo = (): boolean =>
  typeof window === "undefined" || window.__CONCORDIA_NATIVE_INPUT__ !== true;

/** The viewport's size in whole CSS px (at least 16): frames are presented at this size × scale. */
export function slotSize(box: { width: number; height: number }): { width: number; height: number } {
  return { width: Math.max(16, Math.round(box.width)), height: Math.max(16, Math.round(box.height)) };
}

/** The slot as the shell takes it: position, `slotSize`, scale and the page's height. A native
 * window must match the presented frames exactly (an X11 Vulkan surface is its window's size). */
export function slotRect(
  b: { left: number; top: number; width: number; height: number },
  scale: number,
  viewportHeight: number,
) {
  return { x: b.left, y: b.top, ...slotSize(b), scale, viewport_height: viewportHeight };
}

/** Tells the shell where the slot is (CSS px from the page's top-left). */
export async function reportSlot(element: HTMLElement): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  const rect = slotRect(element.getBoundingClientRect(), window.devicePixelRatio, window.innerHeight);
  if (Object.values(rect).every((v) => Number.isFinite(v))) await invoke("slot_rect", { rect });
}

/** Calls `onView` with each editor-camera view the shell's native input produces. */
export function onNativeView(onView: (view: import("./camera").View) => void): () => void {
  let stop: (() => void) | undefined;
  let stopped = false;
  void import("@tauri-apps/api/event").then(({ listen }) =>
    listen<import("./camera").View>("viewport-view", (e) => onView(e.payload)).then((unlisten) => {
      if (stopped) unlisten();
      else stop = unlisten;
    }),
  );
  return () => {
    stopped = true;
    stop?.();
  };
}
