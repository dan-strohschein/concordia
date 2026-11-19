// Pending approvals (ADR-0083): tasks in review. Approve (comment optional) or reject (reason
// required) as the local human; the engine checks the team's gates.
import { useState } from "react";

import type { EditorSession } from "../engine/session";
import { approveTask, rejectTask, type StudioState, type TaskView } from "./studio";

export function Approvals({ session, studio }: { session: EditorSession; studio: StudioState }) {
  const pending = studio.tasks.filter((t) => t.status === "in_review");
  return (
    <ul aria-label="Pending approvals" style={{ listStyle: "none", margin: 0, padding: 0 }}>
      {pending.map((t) => (
        <Pending key={t.id} session={session} task={t} onDone={studio.reload} />
      ))}
    </ul>
  );
}

function Pending({ session, task, onDone }: { session: EditorSession; task: TaskView; onDone: () => void }) {
  const [comment, setComment] = useState("");
  const [error, setError] = useState<string>();
  const run = (call: Promise<void>) =>
    call.then(onDone, (e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  return (
    <li style={{ padding: 4, margin: "2px 0", background: "#2b2b2b" }}>
      <div>
        <strong>{task.id}</strong> {task.title} <span style={{ opacity: 0.7 }}>({task.assignee ?? task.role})</span>
      </div>
      {task.acceptance.map((a) => (
        <div key={a.assert} style={{ opacity: 0.7 }}>
          ✓ {a.assert}
        </div>
      ))}
      <div style={{ display: "flex", gap: 4, marginTop: 4 }}>
        <input
          aria-label={`Comment on ${task.id}`}
          placeholder="What you checked, or what is wrong"
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          style={{ flex: 1 }}
        />
        <button type="button" onClick={() => void run(approveTask(session, task.id, comment))}>
          Approve
        </button>
        <button
          type="button"
          disabled={comment.trim() === ""}
          onClick={() => void run(rejectTask(session, task.id, comment))}
        >
          Reject
        </button>
      </div>
      {error && (
        <p role="alert" style={{ color: "#f88", margin: 0 }}>
          {error}
        </p>
      )}
    </li>
  );
}
