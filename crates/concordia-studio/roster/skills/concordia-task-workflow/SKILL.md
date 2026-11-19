---
name: concordia-task-workflow
description: "How every Concordia Studio role works a task on the board: claim it, do the work through the engine tools, record evidence, and hand it to review. Covers the task statuses, comments and what reviewers expect."
when_to_use: "Use whenever you start, continue or finish a task on the Studio task board (ids like T-0007)."
user-invocable: false
---
# Working a task

Every piece of Studio work is a task on the board (`studio/tasks/T-0001-<slug>.task.md`). Change tasks only through the `task_*` tools, never by editing task files: a direct edit is logged as a hand edit by a human.

## Statuses
`todo` → `in_progress` (you claimed it) → `in_review` (your work is ready) → `done` (a reviewer approved it) or `rejected` (back to you with a reason). After too many rejections a task becomes `blocked` for a human.

## Steps
1. **Read it.** `mcp__concordia__task_get {"id": "T-0007"}` returns the task and its history. Read the description, every acceptance criterion and earlier review comments. If you were sent a rejected task, the latest rejection comment says what to fix.
2. **Claim it.** `mcp__concordia__task_claim {"id": "T-0007"}`. It must be assigned to your role. If the claim fails, report why to whoever delegated to you; don't work around it.
3. **Do the work** inside your role's tools and write paths. Group related engine edits in one transaction (`txn_begin` … `txn_commit`) so they undo as one step, and save scenes you changed.
4. **Check every acceptance criterion yourself** before asking for review. Collect evidence: query results, entity ids, screenshots, simulation outcomes, test output.
5. **Record evidence.** `mcp__concordia__task_comment {"id": "T-0007", "text": "..."}` with what you did and how each criterion is met (see the output contract below).
6. **Hand it in.** `mcp__concordia__task_update {"id": "T-0007", "status": "in_review"}`.

## Output contract (the review comment)
One comment, in this shape:
```
Done: <one line>
Changed: <files, scenes or entities, with ids>
Acceptance:
- <criterion 1>: met; <evidence>
- <criterion 2>: met; <evidence>
Notes: <anything a reviewer must know, or "none">
```

## Rules
- One task at a time. Don't start work that has no task; ask the director to create one.
- Never mark your own task done; reviewers do (`task_approve` / `task_reject`).
- If you're stuck after two honest attempts, comment what you tried and what blocks you, and tell whoever delegated to you. Don't guess at design decisions.
- You never need an API key; the engine and Claude Code run on the user's subscription.
