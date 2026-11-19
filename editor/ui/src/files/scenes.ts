// Scene commands for the Scene menu and the FileSystem dock: New, Open, Save and Save As, each
// guarded against losing unsaved changes (Godot's "Save changes before closing?").
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import type { RecentScenes } from "./recent";
import type { SceneSummary } from "./title";

export type UnsavedChoice = "save" | "discard" | "cancel";

/** Asks the user what to do with unsaved changes to the scene named `scene`. */
export type AskUnsaved = (scene: string) => Promise<UnsavedChoice>;

export class SceneFiles {
  constructor(
    private readonly session: EditorSession,
    private readonly recent: RecentScenes,
    private readonly ask: AskUnsaved,
  ) {}

  /** The open scene, or undefined when none is open. */
  async info(): Promise<SceneSummary | undefined> {
    try {
      return await this.session.call<SceneSummary>("scene.info");
    } catch {
      return undefined;
    }
  }

  /** Creates and opens a new scene at `path`; false if the user cancelled. */
  async newScene(path: string, name: string): Promise<boolean> {
    return this.replace("scene.new", { path, name, actor: EDITOR_ACTOR });
  }

  /** Opens the scene at `path`; false if the user cancelled. */
  async open(path: string): Promise<boolean> {
    return this.replace("scene.open", { path });
  }

  /** Saves the open scene to its own path. */
  async save(): Promise<void> {
    await this.session.call("scene.save");
  }

  /** Saves the open scene to `path`, which becomes its path. */
  async saveAs(path: string): Promise<void> {
    await this.session.call("scene.save", { path });
    this.recent.add(path);
  }

  /** Whether the open scene may be closed: true when clean, saved, or discarded by the user. */
  async mayClose(): Promise<boolean> {
    return (await this.guard()) !== "cancel";
  }

  private async replace(method: string, params: Record<string, unknown>): Promise<boolean> {
    const choice = await this.guard();
    if (choice === "cancel") return false;
    await this.session.call(method, { ...params, discard: choice === "discard" });
    this.recent.add(String(params["path"]));
    return true;
  }

  /** "save" or "discard" when it is safe to replace the scene ("save" also when clean). */
  private async guard(): Promise<UnsavedChoice> {
    const scene = await this.info();
    if (scene === undefined || !scene.dirty) return "save";
    const choice = await this.ask(scene.name);
    if (choice === "save") await this.save();
    return choice;
  }
}
