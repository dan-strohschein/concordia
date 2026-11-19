// The Scene menu and its shortcuts (Godot's: ⌘N new, ⌘O open, ⌘S save, ⌘⇧S save as, ⌘Q quit),
// the dialogs they open, the window title, and the guard against closing with unsaved changes.
import { useEffect, useMemo, useRef, useState } from "react";

import type { EditorSession } from "../engine/session";
import { PathDialog, type PathPrompt, UnsavedDialog } from "../files/dialogs";
import { browserStore, RecentScenes } from "../files/recent";
import { SceneFiles, type UnsavedChoice } from "../files/scenes";
import { windowTitle } from "../files/title";
import { baseName, type ProjectFile } from "../files/tree";
import type { Menu } from "./MenuBar";
import { closeWindow, guardClose, setWindowTitle } from "./window";

interface Pending<T, R> {
  value: T;
  resolve: (result: R) => void;
}

/** A scene's display name from its path: "scenes/my_level.scene.ron" → "my_level". */
const sceneName = (path: string) => baseName(path).replace(/\.scene\.ron$/, "");

export function useSceneMenu(session: EditorSession) {
  const [unsaved, setUnsaved] = useState<Pending<string, UnsavedChoice>>();
  const [prompt, setPrompt] = useState<Pending<PathPrompt, string | undefined>>();
  const [error, setError] = useState<string>();
  const [, setVersion] = useState(0);
  const dirty = useRef(false);
  const recent = useMemo(() => new RecentScenes(browserStore()), []);
  const files = useMemo(
    () => new SceneFiles(session, recent, (scene) => new Promise((resolve) => setUnsaved({ value: scene, resolve }))),
    [session, recent],
  );
  const ask = (value: PathPrompt) => new Promise<string | undefined>((resolve) => setPrompt({ value, resolve }));
  const attempt = (work: () => Promise<unknown>) => {
    setError(undefined);
    work()
      .then(() => setVersion((v) => v + 1))
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  };

  const commands = {
    newScene: () =>
      attempt(async () => {
        const path = await ask({ title: "New Scene", initial: "scenes/new_scene.scene.ron", confirm: "Create" });
        if (path !== undefined) await files.newScene(path, sceneName(path));
      }),
    open: () =>
      attempt(async () => {
        const listing = await session.call<{ files: ProjectFile[] }>("project.files");
        const scenes = listing.files.filter((f) => f.kind === "scene").map((f) => f.path);
        const path = await ask({ title: "Open Scene", initial: scenes[0] ?? "", suggestions: scenes, confirm: "Open" });
        if (path !== undefined) await files.open(path);
      }),
    openPath: (path: string) => attempt(() => files.open(path)),
    save: () => attempt(() => files.save()),
    saveAs: () =>
      attempt(async () => {
        const current = await files.info();
        const path = await ask({
          title: "Save Scene As",
          initial: current?.path ?? "scenes/new_scene.scene.ron",
          confirm: "Save",
        });
        if (path !== undefined) await files.saveAs(path);
      }),
    quit: () =>
      attempt(async () => {
        if (await files.mayClose()) closeWindow();
      }),
  };

  useTitle(session, files, dirty);
  useEffect(
    () =>
      guardClose(
        () => files.mayClose(),
        () => dirty.current,
      ),
    [files],
  );
  useShortcuts(commands);

  const recentPaths = recent.list();
  const menu: Menu = {
    label: "Scene",
    items: [
      { label: "New Scene…", shortcut: "⌘N", run: commands.newScene },
      { label: "Open Scene…", shortcut: "⌘O", run: commands.open },
      "separator",
      ...(recentPaths.length === 0
        ? [{ label: "No Recent Scenes", enabled: false, run: () => undefined }]
        : recentPaths.map((path) => ({ label: `Open Recent: ${path}`, run: () => commands.openPath(path) }))),
      "separator",
      { label: "Save Scene", shortcut: "⌘S", run: commands.save },
      { label: "Save Scene As…", shortcut: "⌘⇧S", run: commands.saveAs },
      "separator",
      { label: "Quit", shortcut: "⌘Q", run: commands.quit },
    ],
  };

  const dialogs = (
    <>
      {unsaved && (
        <UnsavedDialog
          scene={unsaved.value}
          onChoose={(choice) => {
            setUnsaved(undefined);
            unsaved.resolve(choice);
          }}
        />
      )}
      {prompt && (
        <PathDialog
          prompt={prompt.value}
          onSubmit={(path) => {
            setPrompt(undefined);
            prompt.resolve(path);
          }}
          onCancel={() => {
            setPrompt(undefined);
            prompt.resolve(undefined);
          }}
        />
      )}
    </>
  );
  return { menu, dialogs, error, openScene: commands.openPath };
}

/** Keeps the window title (and `dirty`) in step with the open scene. */
function useTitle(session: EditorSession, files: SceneFiles, dirty: React.MutableRefObject<boolean>) {
  useEffect(() => {
    let live = true;
    const update = () =>
      void files.info().then((info) => {
        if (!live) return;
        dirty.current = info?.dirty ?? false;
        setWindowTitle(windowTitle(info));
      });
    update();
    const stop = session.onEvent((e) => {
      if (e.topic === "edit" || e.topic === "scene") update();
    });
    return () => {
      live = false;
      stop();
    };
  }, [session, files, dirty]);
}

/** The Scene menu's shortcuts (⌘ or Ctrl). */
function useShortcuts(commands: {
  newScene: () => void;
  open: () => void;
  save: () => void;
  saveAs: () => void;
  quit: () => void;
}) {
  const latest = useRef(commands);
  useEffect(() => {
    latest.current = commands;
  });
  useEffect(() => {
    const keys = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.target instanceof HTMLInputElement) return;
      const run = {
        n: latest.current.newScene,
        o: latest.current.open,
        s: e.shiftKey ? latest.current.saveAs : latest.current.save,
        q: latest.current.quit,
      }[e.key.toLowerCase()];
      if (run === undefined) return;
      e.preventDefault();
      run();
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, []);
}
