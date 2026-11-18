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
