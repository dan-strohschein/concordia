// The task board (ADR-0083): one column per status. Dropping a card asks the engine to move it
// (task.update, or task.approve / task.reject for done / rejected) as the local human. The board
// never moves a card itself: it shows what the engine says after the call, so a refused move
// snaps back and its reason is shown.
import { useRef, useState } from "react";

import type { EditorSession } from "../engine/session";
import { RoleBadge } from "./RoleBadge";
import { moveTask, roleColors, STATUSES, type StudioState, type TaskStatus, type TaskView } from "./studio";

const LABELS: Record<TaskStatus, string> = {
  todo: "To do",
  in_progress: "In progress",
  in_review: "In review",
  done: "Done",
  blocked: "Blocked",
  rejected: "Rejected",
};

interface Props {
  session: EditorSession;
  studio: StudioState;
}

export function TaskBoard({ session, studio }: Props) {
  const dragged = useRef<string | undefined>(undefined);
  const [error, setError] = useState<string>();
  const [rejecting, setRejecting] = useState<string>();
  const colors = roleColors(studio.roles);
  const move = (id: string, to: TaskStatus, comment?: string) => {
    setError(undefined);
    moveTask(session, id, to, comment).then(studio.reload, (e: unknown) => {
      setError(e instanceof Error ? e.message : String(e));
      studio.reload();
    });
  };
  const drop = (to: TaskStatus) => {
    const id = dragged.current;
    dragged.current = undefined;
    const from = studio.tasks.find((t) => t.id === id)?.status;
    if (id === undefined || from === to) return;
    if (to === "rejected") setRejecting(id);
    else move(id, to);
  };
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4, height: "100%" }}>
      {error && (
        <p role="alert" style={{ color: "#f88", margin: 0 }}>
          {error}
        </p>
      )}
      {rejecting !== undefined && (
        <RejectForm
          id={rejecting}
          onReject={(comment) => {
            setRejecting(undefined);
            move(rejecting, "rejected", comment);
          }}
          onCancel={() => setRejecting(undefined)}
        />
      )}
      <section aria-label="Task board" style={{ display: "flex", gap: 6, flex: 1, minHeight: 0 }}>
        {STATUSES.map((status) => (
          <div
            key={status}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              drop(status);
            }}
            style={{ flex: 1, minWidth: 0, background: "#1e1e1e", padding: 4, overflow: "auto" }}
          >
            <div style={{ fontWeight: 600, opacity: 0.8 }}>{LABELS[status]}</div>
            <ul aria-label={status} style={{ listStyle: "none", margin: 0, padding: 0, minHeight: 40 }}>
              {studio.tasks
                .filter((t) => t.status === status)
                .map((t) => (
                  <Card key={t.id} task={t} color={colors[t.role]} onDragStart={() => (dragged.current = t.id)} />
                ))}
            </ul>
          </div>
        ))}
      </section>
    </div>
  );
}

function Card({ task, color, onDragStart }: { task: TaskView; color: string | undefined; onDragStart: () => void }) {
  const evidence = task.notes.at(-1)?.replace(/^- /, "");
  return (
    <li
      aria-label={`${task.id} ${task.title}`}
      draggable
      onDragStart={(e) => {
        e.dataTransfer?.setData("text/plain", task.id);
        onDragStart();
      }}
      style={{
        margin: "4px 0",
        padding: 4,
        background: "#2b2b2b",
        borderLeft: `3px solid ${color ?? "#666"}`,
        cursor: "grab",
      }}
    >
      <div>
        <strong>{task.id}</strong> {task.title}
      </div>
      <div style={{ opacity: 0.75 }}>
        {task.role}
        {task.assignee !== null && (
          <>
            {" · "}
            <RoleBadge actor={task.assignee} color={color} />
          </>
        )}
      </div>
      {task.acceptance.map((a) => (
        <div key={a.assert} style={{ opacity: 0.7 }}>
          ✓ {a.assert}
        </div>
      ))}
      {evidence !== undefined && <div style={{ opacity: 0.6, fontStyle: "italic" }}>{evidence}</div>}
    </li>
  );
}

function RejectForm({
  id,
  onReject,
  onCancel,
}: {
  id: string;
  onReject: (comment: string) => void;
  onCancel: () => void;
}) {
  const [comment, setComment] = useState("");
  return (
    <form
      aria-label={`Reject ${id}`}
      onSubmit={(e) => {
        e.preventDefault();
        if (comment.trim() !== "") onReject(comment);
      }}
      style={{ display: "flex", gap: 4, alignItems: "center" }}
    >
      <span>Reject {id}: what is wrong?</span>
      <input aria-label="Reason" value={comment} onChange={(e) => setComment(e.target.value)} style={{ flex: 1 }} />
      <button type="submit" disabled={comment.trim() === ""}>
        Reject
      </button>
      <button type="button" onClick={onCancel}>
        Cancel
      </button>
    </form>
  );
}
