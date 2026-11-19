// Native-shell mode (ADR-0043 as amended): the Metal view sits below the transparent page at the
// viewport slot. The page reports the slot to Rust and asks the engine to present frames.

declare global {
  interface Window {
    /** Set by the native shell. */
    __CONCORDIA_NATIVE__?: boolean;
  }
}

/** True inside the native editor shell. */
export const isNative = (): boolean => typeof window !== "undefined" && window.__CONCORDIA_NATIVE__ === true;

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

/** A gizmo handle the shell can hit-test natively (viewport pixels). */
export interface GizmoHandle {
  axis: "x" | "y" | "z";
  x: number;
  y: number;
  radius: number;
}

/** Tells the shell where the selected entity's gizmo handles are (none to clear). */
export function reportGizmoHandles(entity: string, handles: GizmoHandle[]): void {
  void import("@tauri-apps/api/core").then(({ invoke }) =>
    invoke("gizmo_handles", { entity, handles }).catch(() => undefined),
  );
}
