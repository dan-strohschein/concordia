// The editor's Engine API session: every edit is an API call attributed to `human:local`.
import type { EngineClient, EngineEvent } from "../api/client";

/** The actor every editor edit is attributed to. */
export const EDITOR_ACTOR = "human:local";

/** An entity as `entity.get` returns it. */
export interface EntityData {
  id: string;
  components: Record<string, unknown>;
}

/** A `query.run` row. */
export interface Row {
  id: string;
  components: Record<string, unknown>;
}

export class EditorSession {
  private readonly schemas = new Map<string, Promise<unknown>>();

  private constructor(readonly client: EngineClient) {}

  /** Opens a session as `human:local` and subscribes to edit, scene, sim, pending, log and studio events. */
  static async open(client: EngineClient): Promise<EditorSession> {
    await client.call("session.open", { client: "concordia-editor", actor: EDITOR_ACTOR });
    await client.call("events.subscribe", { topics: ["edit", "scene", "sim", "pending", "log", "studio"] });
    return new EditorSession(client);
  }

  /** All entities with their Name and Parent (for the Outliner). */
  async entities(): Promise<Row[]> {
    const rows: Row[] = [];
    let cursor: string | undefined;
    do {
      const page = await this.client.call<{ rows: Row[]; next_cursor?: string | null }>("query.run", {
        with: [],
        select: ["Name", "Parent"],
        limit: 1000,
        ...(cursor === undefined ? {} : { cursor }),
      });
      rows.push(...page.rows);
      cursor = page.next_cursor ?? undefined;
    } while (cursor !== undefined);
    return rows;
  }

  /** The Outliner rows (Name and Parent) of `ids`; `undefined` for entities that are gone. */
  async entityRows(ids: readonly string[]): Promise<{ id: string; row: Row | undefined }[]> {
    return Promise.all(
      ids.map(async (id) => {
        try {
          const e = await this.entity(id);
          const { Name, Parent } = e.components;
          const components = { ...(Name === undefined ? {} : { Name }), ...(Parent === undefined ? {} : { Parent }) };
          return { id, row: { id, components } };
        } catch {
          return { id, row: undefined };
        }
      }),
    );
  }

  entity(id: string): Promise<EntityData> {
    return this.client.call<EntityData>("entity.get", { id });
  }

  /** A type's JSON Schema (cached for the session). */
  schema(name: string): Promise<unknown> {
    let schema = this.schemas.get(name);
    if (schema === undefined) {
      schema = this.client.call<{ schema: unknown }>("schema.get", { name }).then((r) => r.schema);
      this.schemas.set(name, schema);
    }
    return schema;
  }

  /** Replaces a component value (one undo step). */
  setComponent(entity: string, component: string, value: unknown): Promise<unknown> {
    return this.client.call("component.set", { entity, component, value, actor: EDITOR_ACTOR });
  }

  /** Any API call (toolbar actions, sim control, capture). */
  call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    return this.client.call<T>(method, params);
  }

  /** Listens to every pushed event; returns the unsubscribe function. */
  onEvent(listener: (event: EngineEvent) => void): () => void {
    return this.client.onEvent(listener);
  }
}
