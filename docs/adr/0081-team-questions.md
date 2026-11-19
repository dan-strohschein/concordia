# ADR-0081: Team questions and answers, routed by the Director
- Status: Accepted
- Date: 2026-11-19
- Builds on: ADR-0049 (Studio right after M1), ADR-0080 (Studio attribution and the Studio journal)

## Context
The author wants Studio agents to ask each other questions and get answers. A human should be able to see who asked what and when, and who answered what and when. Messages should use as few tokens as possible while staying readable. The producer (the Director) escalates what it can't answer to the human. All of this applies only in team mode.

Claude Code sets the constraints (docs checked against 2.1.328):
- **Subagents can't message each other.** Only the main session spawns subagents, and each returns once to it.
- **"Agent teams" isn't usable here.** That feature allows direct messages, but it is experimental and doesn't run under `claude -p`.
- **A subagent can't block waiting for an answer.** Its parent is waiting for it to return, so it would deadlock.

## Decision
1. **Questions are records on the Studio board, routed by the Director.**
   - **Methods** (Studio module): `studio.ask {to, text, task?, refs?}` gives an id `Q-0001`. The others are `studio.answer {id, text}`, `studio.escalate {id, reason}` and `studio.questions {...}`.
   - **Storage:** the journal events `question_asked`, `question_answered` and `question_escalated`. State is derived from the journal; there are no question files.
   - **The asker** either continues, or returns to the Director reporting that it's blocked on that question.
   - **The Director** answers the question itself, or spawns the addressed role to answer it, then re-runs the asker.
2. **Who may answer:** an agent of the addressed role, the Director or a human. An escalated question may be answered only by a human. The first answer closes a question.
3. **Escalation:**
   - Only the Director escalates, and only when it can't answer and no role can. An escalated question is addressed to `human`.
   - Its task, if any, becomes blocked (attributed to `system:studio`).
   - A human answer unblocks the task to `todo`.
   - The editor's **Team Questions** panel is the escalation channel. Escalations are pinned, counted on a badge, and announced with a banner that stays up until answered. The panel reveals itself when one arrives. There is no OS notification.
4. **Getting answers to agents:** hooks inject context, so nobody polls.
   - SubagentStart gives a role the open questions addressed to it, plus answers to its own questions.
   - PostToolUse on the `Agent` tool gives the Director newly opened or answered questions when a subagent returns.
   - UserPromptSubmit gives an interactive Director the human answers since its last prompt.
5. **Token-lean messages:** structured fields carry the context (from, to, task, refs by id), and the text is telegraphic English with a small glossary from the `concordia-team-comms` skill. The schema caps a question at 280 characters, an answer at 500 and an escalation reason at 280.
6. **Team mode only:** the methods exist only with the Studio module (`[studio]`), only team roles get them in scope, and the panel appears only in Studio projects. *Assumption:* team mode means Studio is on in the project; a per-project solo switch is not built.
7. **Tested without live agents:** unit, API, hook, CLI, scripted MCP, editor unit and Playwright tests. The live gate is unchanged and optional.

## Alternatives
- **Agent teams with direct messaging:** experimental and unavailable in `-p`. Revisit when it's stable.
- **A blocking `studio.ask` that waits for an answer:** it deadlocks the sequential subagent model.
- **A coded vocabulary instead of telegraphic English:** the extra savings are small next to ids and structure, and it costs model accuracy and human readability.
- **Question files under `studio/questions/`:** questions are activity, not configuration, so the journal is the right text record.

## Consequences
- Every exchange is attributed and timestamped in the Studio journal, shown live in the editor and queryable over the API and CLI.
- The Director's prompt and the comms skill carry the routing and escalation rules; Phase 3.6 writes those prompts.
- The editor's view of live agent questions depends on the shared engine (3.5b). Without it, the panel still shows the journal on refresh.
