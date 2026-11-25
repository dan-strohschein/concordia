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

/** Tells the shell where the slot is (CSS px from the page's top-left). */
export async function reportSlot(element: HTMLElement): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  const b = element.getBoundingClientRect();
  const rect = {
    x: b.left,
    y: b.top,
    width: b.width,
    height: b.height,
    scale: window.devicePixelRatio,
    viewport_height: window.innerHeight,
  };
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
