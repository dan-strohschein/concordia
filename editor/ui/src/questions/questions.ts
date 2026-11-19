// Team questions (ADR-0081): the editor's view of studio.questions, refreshed by `studio` events.
import { useEffect, useMemo, useState } from "react";

import { JsonRpcError } from "../api/client";
import { Coalescer } from "../engine/coalesce";
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";

/** Longest answer the engine accepts (`studio.answer`). */
export const ANSWER_LIMIT = 500;

/** A `studio.questions` entry. */
export interface Question {
  id: string;
  from: string;
  to: string;
  task: string | null;
  refs: string[];
  text: string;
  asked_ms: number;
  status: "open" | "answered" | "escalated";
  answer?: { by: string; text: string; ms: number } | null;
  escalation?: { by: string; reason: string; ms: number } | null;
}

/** The panel's shared state: whether the project has Agent Studio, and its questions. */
export interface TeamState {
  teamMode: boolean;
  questions: Question[];
  /** Re-reads the questions now. */
  reload: () => void;
}

/** JSON-RPC "method not found": the engine has no Studio module. */
const METHOD_NOT_FOUND = -32601;

/** The questions, or `undefined` outside team mode. */
export async function loadQuestions(session: EditorSession): Promise<Question[] | undefined> {
  try {
    return (await session.call<{ questions: Question[] }>("studio.questions", {})).questions;
  } catch (e) {
    if (e instanceof JsonRpcError && e.code === METHOD_NOT_FOUND) return undefined;
    throw e;
  }
}

/** Escalated questions, oldest first: they wait for the human. */
export function escalated(questions: readonly Question[]): Question[] {
  return questions.filter((q) => q.status === "escalated");
}

/** Escalated questions pinned first, then the rest; each group in id order. */
export function pinned(questions: readonly Question[]): Question[] {
  return [...escalated(questions), ...questions.filter((q) => q.status !== "escalated")];
}

/** The role of an actor (`agent:qa-playtester#1` → `qa-playtester`). */
export function roleOf(actor: string): string {
  return actor.replace(/^[a-z]+:/, "").replace(/#\d+$/, "");
}

/** Answers `id` as the local human. */
export async function answerQuestion(session: EditorSession, id: string, text: string): Promise<void> {
  if (text.trim() === "" || text.length > ANSWER_LIMIT) {
    throw new RangeError(`an answer needs 1 to ${ANSWER_LIMIT} characters`);
  }
  await session.call("studio.answer", { id, text, actor: EDITOR_ACTOR });
}

/** Loads the questions and keeps them current from `studio` events. */
export function useTeamQuestions(session: EditorSession): TeamState {
  const [state, setState] = useState<{ teamMode: boolean; questions: Question[] }>({
    teamMode: false,
    questions: [],
  });
  const loader = useMemo(
    () =>
      new Coalescer(async () => {
        const questions = await loadQuestions(session);
        setState({ teamMode: questions !== undefined, questions: questions ?? [] });
      }),
    [session],
  );
  useEffect(() => {
    loader.request();
    return session.onEvent((e) => (e.topic === "studio" ? loader.request() : undefined));
  }, [session, loader]);
  return { ...state, reload: () => loader.request() };
}
