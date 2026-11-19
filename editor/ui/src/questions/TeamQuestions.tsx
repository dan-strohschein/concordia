// The Team Questions dock (ADR-0081): who asked what and when, who answered what and when.
// Escalations are pinned, counted on a badge and announced by a banner that stays until every one
// is answered; a new escalation reveals the dock. Answers go to studio.answer as the local human.
import { useEffect, useRef, useState } from "react";

import type { EditorSession } from "../engine/session";
import { ANSWER_LIMIT, answerQuestion, escalated, pinned, type Question, roleOf, type TeamState } from "./questions";

const time = (ms: number) => new Date(ms).toLocaleTimeString();
const title: React.CSSProperties = { padding: "3px 8px", background: "#2d2d2d", fontWeight: 600 };
const badgeStyle: React.CSSProperties = {
  marginLeft: 6,
  padding: "0 6px",
  borderRadius: 8,
  background: "#c0392b",
  color: "#fff",
  fontSize: 11,
};

/** The count of escalated questions (nothing when there are none). */
export function EscalationBadge({ team }: { team: TeamState }) {
  const count = team.teamMode ? escalated(team.questions).length : 0;
  if (count === 0) return null;
  return (
    <span aria-label={`${count} escalated question${count === 1 ? "" : "s"}`} style={badgeStyle}>
      {count}
    </span>
  );
}

interface DeskProps {
  session: EditorSession;
  team: TeamState;
  /** Whether the dock is shown (View menu). */
  shown: boolean;
  /** Shows the dock. */
  onReveal: () => void;
}

/** The escalation banner and, when shown, the dock; nothing outside team mode. */
export function TeamDesk({ session, team, shown, onReveal }: DeskProps) {
  const waiting = escalated(team.questions);
  const [focus, setFocus] = useState<string>();
  const seen = useRef<Set<string> | undefined>(undefined);
  useEffect(() => {
    const ids = new Set(waiting.map((q) => q.id));
    const fresh = seen.current === undefined ? [] : [...ids].filter((id) => !seen.current?.has(id));
    seen.current = ids;
    if (fresh[0] !== undefined) {
      onReveal();
      if (!typingAnAnswer()) setFocus(fresh[0]);
    }
  }, [waiting, onReveal]);
  if (!team.teamMode) return null;
  const answer = (id: string) => {
    onReveal();
    setFocus(id);
  };
  return (
    <>
      {waiting.length > 0 && <Banner first={waiting[0]} count={waiting.length} onAnswer={answer} />}
      {shown && (
        <section style={{ background: "#232323", overflow: "auto", minHeight: 0 }} aria-label="Team Questions dock">
          <div style={title}>
            Team Questions
            <EscalationBadge team={team} />
          </div>
          <Threads session={session} team={team} focus={focus} />
        </section>
      )}
    </>
  );
}

/** True while the user types in an answer box (a new escalation must not steal the caret). */
function typingAnAnswer(): boolean {
  const active = document.activeElement;
  return active instanceof HTMLTextAreaElement && active.dataset["answerBox"] !== undefined;
}

function Banner({
  first,
  count,
  onAnswer,
}: {
  first: Question | undefined;
  count: number;
  onAnswer: (id: string) => void;
}) {
  if (first === undefined) return null;
  return (
    <div
      role="alert"
      aria-label="Escalated questions"
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        zIndex: 900,
        display: "flex",
        gap: 12,
        alignItems: "center",
        justifyContent: "center",
        padding: "6px 16px",
        background: "#8e5a00",
        color: "#fff",
        font: "13px system-ui",
      }}
    >
      <span>
        The producer needs you: {first.id} {first.text}
        {count > 1 && ` (+${count - 1} more)`}
      </span>
      <button type="button" onClick={() => onAnswer(first.id)}>
        Answer
      </button>
    </div>
  );
}

function Threads({ session, team, focus }: { session: EditorSession; team: TeamState; focus: string | undefined }) {
  const [status, setStatus] = useState<string>("");
  const [role, setRole] = useState<string>("");
  const roles = [...new Set(team.questions.flatMap((q) => [roleOf(q.from), q.to]))].sort();
  const rows = pinned(team.questions).filter(
    (q) => (status === "" || q.status === status) && (role === "" || roleOf(q.from) === role || q.to === role),
  );
  return (
    <div style={{ padding: 4, fontSize: 12 }}>
      <div style={{ display: "flex", gap: 8 }}>
        <label>
          Status{" "}
          <select value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="">All</option>
            <option value="escalated">Escalated</option>
            <option value="open">Open</option>
            <option value="answered">Answered</option>
          </select>
        </label>
        <label>
          Role{" "}
          <select value={role} onChange={(e) => setRole(e.target.value)}>
            <option value="">All</option>
            {roles.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </label>
      </div>
      <ul aria-label="Questions" style={{ listStyle: "none", margin: "4px 0", padding: 0 }}>
        {rows.map((q) => (
          <Thread key={q.id} session={session} question={q} focused={q.id === focus} onAnswered={team.reload} />
        ))}
      </ul>
    </div>
  );
}

interface ThreadProps {
  session: EditorSession;
  question: Question;
  focused: boolean;
  onAnswered: () => void;
}

function Thread({ session, question: q, focused, onAnswered }: ThreadProps) {
  const item = useRef<HTMLLIElement>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (!focused) return;
    item.current?.scrollIntoView?.({ block: "nearest" });
    box.current?.focus();
  }, [focused]);
  const escalatedNow = q.status === "escalated";
  return (
    <li
      ref={item}
      data-question={q.id}
      style={{
        padding: "4px 6px",
        margin: "2px 0",
        borderLeft: `3px solid ${escalatedNow ? "#e67e22" : q.status === "open" ? "#3498db" : "#555"}`,
      }}
    >
      <div>
        <strong>{q.id}</strong> {q.from} → {q.to}
        {q.task !== null && <span style={{ opacity: 0.7 }}> · {q.task}</span>}{" "}
        <span style={{ opacity: 0.5 }}>{time(q.asked_ms)}</span>
      </div>
      <div style={{ margin: "2px 0" }}>{q.text}</div>
      {q.escalation && (
        <div style={{ color: "#e6a04a" }}>
          escalated by {q.escalation.by}: {q.escalation.reason}{" "}
          <span style={{ opacity: 0.5 }}>{time(q.escalation.ms)}</span>
        </div>
      )}
      {q.answer && (
        <div style={{ color: "#9fd49f" }}>
          {q.answer.by}: {q.answer.text} <span style={{ opacity: 0.5 }}>{time(q.answer.ms)}</span>
        </div>
      )}
      {escalatedNow && <AnswerBox session={session} id={q.id} box={box} onAnswered={onAnswered} />}
    </li>
  );
}

interface AnswerBoxProps {
  session: EditorSession;
  id: string;
  box: React.RefObject<HTMLTextAreaElement | null>;
  onAnswered: () => void;
}

function AnswerBox({ session, id, box, onAnswered }: AnswerBoxProps) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string>();
  const send = () =>
    answerQuestion(session, id, text).then(
      () => {
        setText("");
        onAnswered();
      },
      (e: unknown) => setError(String(e)),
    );
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2, marginTop: 4 }}>
      <textarea
        ref={box}
        data-answer-box=""
        aria-label={`Answer ${id}`}
        maxLength={ANSWER_LIMIT}
        rows={2}
        value={text}
        onChange={(e) => setText(e.target.value.slice(0, ANSWER_LIMIT))}
      />
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button type="button" disabled={text.trim() === ""} onClick={() => void send()}>
          Answer
        </button>
        <span style={{ opacity: 0.6 }}>
          {text.length}/{ANSWER_LIMIT}
        </span>
        {error && <span style={{ color: "#f88" }}>{error}</span>}
      </div>
    </div>
  );
}
