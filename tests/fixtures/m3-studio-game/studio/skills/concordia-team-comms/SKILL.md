---
name: concordia-team-comms
description: "How Studio teammates ask each other questions and answer them: when to ask instead of guessing, the telegraphic style, the length caps and the glossary. Questions are records on the board; the director routes them and escalates what nobody can answer to the human."
when_to_use: "Use when you are unsure about something another role owns, when a teammate's question is listed for you, or when you are the director routing questions."
user-invocable: false
---
# Team questions

Questions are records on the Studio board (`Q-0007`), not chat. Asking returns at once with an id. **Don't wait for the answer**: nobody can reply while you are running. Either carry on with what you can do, or stop and tell whoever delegated to you that you are blocked on `Q-0007`. The director routes the question, and the answer reaches you next time you start (in your context).

## When to ask
- You need a fact another role owns: a material's id, a scene's camera, a text key, a gameplay number.
- A task's acceptance criteria are ambiguous and the choice matters.
- You found a problem in someone else's work that blocks yours.

Don't ask what you can find yourself (your own engine tools, `mcp__concordia__task_get`, the project files), and don't ask the human directly unless the director told you to; ask `director` instead.

## How to ask
`mcp__concordia__studio_ask {"to": "tech-artist", "text": "crate material id for T-0004? brushed or painted?", "task": "T-0004"}`
- `to`: a role id (`level-designer`, `qa-playtester`, `director`, ...) or `human`.
- `text`: at most **280** characters. Telegraphic: drop articles and pleasantries, keep nouns and numbers. Refer to ids (`T-0004`, `Q-0002`, entity names) instead of restating context; the reader can look them up.
- `task` and `refs`: the ids the question is about. They are structured, so don't repeat them in the text.
- Ask one thing per question.

## How to answer
`mcp__concordia__studio_answer {"id": "Q-0007", "text": "painted; asset:01a1f460-… (assets/materials/red_painted.material.ron)"}`
- At most **500** characters; same style. Give the fact, then where it lives.
- Answer only what you know. If you don't know, say who might, in one line.
- The first answer closes a question.
- `mcp__concordia__studio_questions {"to": "<your role>", "status": "open"}` lists the questions waiting for you.

## Glossary
- `T-0001` a task; `Q-0001` a question.
- `ok` yes / done; `n/a` not applicable; `tbd` not decided yet.
- `→` leads to / becomes; `≈` about; `?` unsure.
- Units: meters, seconds, degrees unless written.

## For the director
`mcp__concordia__studio_questions {"status": "open"}` lists what waits. For each open question: answer it yourself if you can; otherwise delegate to the addressed role ("Answer Q-0007"); if no role can answer, especially a design decision, escalate with `mcp__concordia__studio_escalate {"id": "Q-0007", "reason": "art direction is a design call"}`. Escalation blocks the question's task until the human answers in the editor.

## Example exchange
```
Q-0007 agent:level-designer#1 → tech-artist (T-0004): floor material? stone or metal plate?
Q-0007 answered by agent:tech-artist#1: stone; assets/materials/stone_floor.material.ron, imported
```
