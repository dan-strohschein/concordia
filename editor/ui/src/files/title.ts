// The window title: the open scene's name, starred while it has unsaved changes.

/** The part of `scene.info` the title and the unsaved guard need. */
export interface SceneSummary {
  name: string;
  path: string;
  dirty: boolean;
}

export function windowTitle(scene: SceneSummary | undefined): string {
  if (scene === undefined) return "Concordia";
  return `${scene.dirty ? "*" : ""}${scene.name} - Concordia`;
}
