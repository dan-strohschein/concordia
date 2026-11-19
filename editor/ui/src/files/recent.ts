// Recently opened scenes, per user, in browser storage. Storage may be missing or refuse access;
// the list then lives only in memory.

/** The part of `Storage` this needs. */
export interface KeyValueStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

const KEY = "concordia.recentScenes";
const MAX = 10;

export class RecentScenes {
  private paths: string[];

  constructor(private readonly store: KeyValueStore | undefined) {
    this.paths = read(store);
  }

  /** Newest first. */
  list(): string[] {
    return [...this.paths];
  }

  /** Moves `path` to the front. */
  add(path: string): void {
    this.paths = [path, ...this.paths.filter((p) => p !== path)].slice(0, MAX);
    try {
      this.store?.setItem(KEY, JSON.stringify(this.paths));
    } catch {
      // Unavailable storage only costs persistence.
    }
  }
}

function read(store: KeyValueStore | undefined): string[] {
  try {
    const parsed: unknown = JSON.parse(store?.getItem(KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((p): p is string => typeof p === "string").slice(0, MAX) : [];
  } catch {
    return [];
  }
}

/** The browser's local storage, if it can be reached. */
export function browserStore(): KeyValueStore | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}
