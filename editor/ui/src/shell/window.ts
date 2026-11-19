// The editor window: its title, and the guard that runs before it closes (the native window's
// close button and Quit, or the browser tab's unload).
import { isNative } from "../viewport/native";

type TauriWindow = Awaited<ReturnType<typeof nativeWindow>>;

async function nativeWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

function withNative(f: (w: TauriWindow) => Promise<unknown>): void {
  if (!isNative()) return;
  nativeWindow()
    .then(f)
    .catch((e: unknown) => console.warn("window:", e));
}

/** Sets the page's and the native window's title. */
export function setWindowTitle(title: string): void {
  document.title = title;
  withNative((w) => w.setTitle(title));
}

/**
 * Before the window closes, asks `mayClose` (native shell) or lets the browser warn while
 * `isDirty()`; returns the function that removes the guard.
 */
export function guardClose(mayClose: () => Promise<boolean>, isDirty: () => boolean): () => void {
  const unload = (e: BeforeUnloadEvent) => {
    if (isDirty()) e.preventDefault();
  };
  let stop: (() => void) | undefined;
  let stopped = false;
  if (isNative()) {
    withNative((w) =>
      w
        .onCloseRequested(async (event) => {
          if (!(await mayClose())) event.preventDefault();
        })
        .then((unlisten) => {
          if (stopped) unlisten();
          else stop = unlisten;
        }),
    );
  } else {
    window.addEventListener("beforeunload", unload);
  }
  return () => {
    stopped = true;
    stop?.();
    window.removeEventListener("beforeunload", unload);
  };
}

/** Closes the editor without asking again (call after the guard agreed). */
export function closeWindow(): void {
  if (isNative()) withNative((w) => w.destroy());
  else window.close();
}
