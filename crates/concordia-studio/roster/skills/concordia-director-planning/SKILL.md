---
name: concordia-director-planning
description: "Turns a game goal into Studio tasks: one role per task, small enough to review, each with acceptance criteria a reviewer can check from engine evidence. Also covers ordering with depends_on and routing finished work to review."
when_to_use: "Use when the human gives the director a goal, feature or bug to plan, or asks you to re-plan the board."
user-invocable: true
argument-hint: "<goal to plan>"
---
# Planning work into tasks

## 1. Understand the goal
Restate the goal in one sentence. List what is unclear. If a design decision is missing (style, scale, rules of play), ask the human before creating tasks; never guess one.

## 2. Check the team and the board
- `mcp__concordia__studio_roster {}`: which roles are enabled, their tools and write paths. Only assign enabled roles.
- `mcp__concordia__task_list {}`: avoid duplicating open work; link to it with `depends_on` instead.
- Finished tasks' evidence (`mcp__concordia__task_get`): what already exists. You never inspect the world yourself; if no task records a fact you need, ask the role that owns it (`mcp__concordia__studio_ask`) or plan a small task to find out.

## 3. Split into tasks
Each task:
- belongs to exactly **one role** (its tools and paths can do all of it);
- is small: one reviewable change, roughly under an hour of agent work;
- has **acceptance criteria** a reviewer can verify from engine evidence (queries, screenshots, simulation), never "looks good";
- names its inputs (scene paths, entity names, asset paths) so the agent doesn't search.

Good acceptance: "an entity named `Ramp` exists with a `Collider`; a sphere dropped at (0, 5, 0) settles below y = 1 within 300 ticks". Bad: "make a nice ramp".

## 4. Create them
`mcp__concordia__task_create {"title": "...", "role": "level-designer", "description": "...", "acceptance": [{"assert": "..."}], "depends_on": ["T-0003"]}`

## 5. Delegate
Start the role's subagent with the Agent tool and pass only the task id and one line of context: "Work task T-0007 (concordia-task-workflow)." The task holds everything else.

## 6. Route to review
When a task reaches `in_review`, delegate the review to the reviewer the team's gates name (usually `qa-playtester`). Rejected tasks go back to their role with the rejection comment. A task blocked after repeated rejections goes to the human with a short summary.

## Output contract
After planning, reply to the human with:
```
Plan: <goal in one line>
Tasks:
- T-0007 level-designer: <title> (after T-0006)
- T-0008 qa-playtester: <title>
Open questions: <list, or "none">
```
