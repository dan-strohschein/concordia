// The FileSystem dock's folder tree, built from `project.files`' flat, project-relative list.

/** A project file as `project.files` lists it. */
export interface ProjectFile {
  path: string;
  kind: "scene" | "prefab" | "model" | "image" | "material" | "other";
  size: number;
  /** The asset id once imported. */
  asset: string | null;
  /** The importer that handles it; null when it cannot be imported. */
  importer: string | null;
}

/** A folder: sub-folders and files, each sorted by name. */
export interface FolderNode {
  name: string;
  /** Project-relative path ("" for the root). */
  path: string;
  folders: FolderNode[];
  files: ProjectFile[];
}

const folder = (name: string, path: string): FolderNode => ({ name, path, folders: [], files: [] });

/** Nests `files` under folders, Godot-style with the root shown as `res://`. */
export function buildTree(files: readonly ProjectFile[]): FolderNode {
  const root = folder("res://", "");
  for (const file of files) {
    const parts = file.path.split("/");
    let node = root;
    for (const part of parts.slice(0, -1)) node = child(node, part);
    node.files.push(file);
  }
  sort(root);
  return root;
}

/** The sub-folder `name` of `node`, created on first use. */
function child(node: FolderNode, name: string): FolderNode {
  let next = node.folders.find((f) => f.name === name);
  if (next === undefined) {
    next = folder(name, node.path === "" ? name : `${node.path}/${name}`);
    node.folders.push(next);
  }
  return next;
}

function sort(node: FolderNode): void {
  node.folders.sort((a, b) => a.name.localeCompare(b.name));
  node.files.sort((a, b) => a.path.localeCompare(b.path));
  node.folders.forEach(sort);
}

/** The last path segment. */
export const baseName = (path: string): string => path.slice(path.lastIndexOf("/") + 1);
