//! `task.*` and `studio.*` Engine API methods.

use std::collections::BTreeMap;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, MethodInfo};
use concordia_command::{Actor, ActorKind};
use concordia_core::id::IdSource;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::claude_code::{SyncAction, SyncOptions, sync};
use crate::gate::GateParty;
use crate::journal::{JournalEntry, JournalEvent};
use crate::module::{board_error, publish, roster, state};
use crate::roster::Roster;
use crate::task::{
    Acceptance, BoardError, NewTask, Priority, Task, TaskBoard, TaskEdit, TaskStatus, counts,
};

/// Every Studio method.
pub(crate) fn all() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<TaskCreate>(),
        MethodInfo::of::<TaskGet>(),
        MethodInfo::of::<TaskList>(),
        MethodInfo::of::<TaskClaim>(),
        MethodInfo::of::<TaskUpdate>(),
        MethodInfo::of::<TaskComment>(),
        MethodInfo::of::<TaskApprove>(),
        MethodInfo::of::<TaskReject>(),
        MethodInfo::of::<StudioRoster>(),
        MethodInfo::of::<StudioStatus>(),
        MethodInfo::of::<StudioActivity>(),
        MethodInfo::of::<StudioSync>(),
    ]
    .into_iter()
    .chain(crate::question_methods::all())
    .collect()
}

/// A task as the API returns it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskView {
    /// `T-0001`.
    pub id: String,
    /// Globally unique id.
    pub uid: String,
    /// Title.
    pub title: String,
    /// Status.
    pub status: TaskStatus,
    /// The role that does it.
    pub role: String,
    /// Who holds it.
    pub assignee: Option<String>,
    /// Priority.
    pub priority: Priority,
    /// Prerequisite task ids.
    pub depends_on: Vec<String>,
    /// Who created it.
    pub created_by: String,
    /// Acceptance criteria.
    pub acceptance: Vec<Acceptance>,
    /// Links.
    pub links: Vec<String>,
    /// Description.
    pub description: String,
    /// Notes (`- <actor>: <text>`).
    pub notes: Vec<String>,
    /// Review verdicts.
    pub review: Vec<String>,
    /// The task file, project-relative.
    pub path: String,
}

/// One task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskResult {
    /// The task after the call.
    pub task: TaskView,
}

/// Create a task on the Studio board for an enabled role (the Director plans work this way).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskCreate;

/// `task.create` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskCreateParams {
    /// Short title, e.g. "Build the arena".
    pub title: String,
    /// The role that should do it (see studio.roster), e.g. "level-designer".
    pub role: String,
    /// What to do, in Markdown.
    pub description: Option<String>,
    /// low, normal (default), high or urgent.
    pub priority: Option<Priority>,
    /// What must be true when it is done; reviewers check these.
    pub acceptance: Option<Vec<Acceptance>>,
    /// Task ids that must be done first.
    pub depends_on: Option<Vec<String>>,
    /// Related links or project paths.
    pub links: Option<Vec<String>>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Get one task and its history (from the Studio journal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskGet;

/// A task id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskIdParams {
    /// Task id, e.g. "T-0001".
    pub id: String,
}

/// `task.get` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskGetResult {
    /// The task.
    pub task: TaskView,
    /// Its journal entries, oldest first (seq, ts_ms, actor, kind and the kind's fields).
    pub history: Vec<Value>,
}

/// List tasks on the Studio board, optionally filtered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskList;

/// `task.list` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskListParams {
    /// Only this status.
    pub status: Option<TaskStatus>,
    /// Only this role.
    pub role: Option<String>,
    /// Only this assignee (an actor).
    pub assignee: Option<String>,
}

/// `task.list` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskListResult {
    /// Tasks by id.
    pub tasks: Vec<TaskView>,
}

/// Claim a todo or rejected task: it becomes in_progress, assigned to you.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskClaim;

/// A task id and the actor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskActorParams {
    /// Task id.
    pub id: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Change a task's fields or status (in_progress → in_review when your work is ready; done and rejected go through task.approve and task.reject).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskUpdate;

/// `task.update` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskUpdateParams {
    /// Task id.
    pub id: String,
    /// New status.
    pub status: Option<TaskStatus>,
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New priority.
    pub priority: Option<Priority>,
    /// New acceptance criteria.
    pub acceptance: Option<Vec<Acceptance>>,
    /// New links.
    pub links: Option<Vec<String>>,
    /// New prerequisites.
    pub depends_on: Option<Vec<String>>,
    /// New assignee (an actor).
    pub assignee: Option<Actor>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Add a note to a task (what you did, evidence, questions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskComment;

/// `task.comment` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskCommentParams {
    /// Task id.
    pub id: String,
    /// The note (one line).
    pub text: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Approve a task in review: in_review → done (only reviewers the team's gates allow).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskApprove;

/// `task.approve` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskApproveParams {
    /// Task id.
    pub id: String,
    /// What you checked.
    pub comment: Option<String>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Reject a task in review with a reason: in_review → rejected (blocked for a human after too many attempts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TaskReject;

/// `task.reject` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TaskRejectParams {
    /// Task id.
    pub id: String,
    /// What is wrong (required).
    pub comment: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// The Studio team: enabled roles with their tools and write paths, and disabled roles with reasons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioRoster;

/// No params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

/// One enabled role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RoleView {
    /// Role id (agent actors are agent:<role>#n).
    pub role: String,
    /// Name.
    pub display_name: String,
    /// When to delegate to it.
    pub description: String,
    /// Instances.
    pub count: u32,
    /// Model hint.
    pub model: String,
    /// Engine MCP tools it may call.
    pub mcp_tools: Vec<String>,
    /// Harness tools (read, search, edit, write, bash).
    pub harness_tools: Vec<String>,
    /// Allowed shell command patterns.
    pub bash_allow: Vec<String>,
    /// Path patterns it may write.
    pub write: Vec<String>,
    /// Path patterns it may read.
    pub read: Vec<String>,
    /// Who must review its tasks.
    pub review: Vec<String>,
    /// Preloaded skills.
    pub skills: Vec<String>,
    /// Turn limit.
    pub max_turns: Option<u32>,
}

/// One disabled role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct DisabledView {
    /// Role id.
    pub role: String,
    /// Why it is off.
    pub reason: String,
}

/// `studio.roster` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioRosterResult {
    /// Team name.
    pub team: String,
    /// The role the main session runs as.
    pub director: String,
    /// The role for calls with no agent type.
    pub main_session_actor: String,
    /// Enabled roles, by id.
    pub roles: Vec<RoleView>,
    /// Disabled roles, by id.
    pub disabled: Vec<DisabledView>,
}

/// Board overview: task counts by status and tasks waiting for approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioStatus;

/// `studio.status` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioStatusResult {
    /// Count per status.
    pub tasks: BTreeMap<String, u32>,
    /// Task ids in review, waiting for a reviewer.
    pub pending_approvals: Vec<String>,
}

/// Studio activity from the Studio journal (task changes, reviews, file edits by agents), oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioActivity;

/// `studio.activity` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StudioActivityParams {
    /// Only entries after this seq (use `next` from the last call).
    pub since: Option<u64>,
    /// Only this actor, e.g. "agent:level-designer#1".
    pub actor: Option<String>,
    /// Only agents in this role, e.g. "level-designer".
    pub role: Option<String>,
    /// Only this task.
    pub task: Option<String>,
    /// Only this kind: task_created, task_changed, task_comment, task_review, file_edit, sync.
    pub kind: Option<String>,
    /// Maximum entries (default 100).
    #[reflect(range(min = 1.0, max = 1000.0))]
    pub limit: Option<u32>,
}

/// `studio.activity` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioActivityResult {
    /// Entries (seq, ts_ms, actor, kind and the kind's fields).
    pub entries: Vec<Value>,
    /// Pass as `since` to continue.
    pub next: u64,
}

fn view(engine_root: &std::path::Path, board: &TaskBoard, task: &Task) -> TaskView {
    let meta = &task.meta;
    let path = board
        .file(task)
        .and_then(|p| {
            p.strip_prefix(engine_root)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    TaskView {
        id: meta.id.clone(),
        uid: meta.uid.clone(),
        title: meta.title.clone(),
        status: meta.status,
        role: meta.role.clone(),
        assignee: meta.assignee.clone(),
        priority: meta.priority,
        depends_on: meta.depends_on.clone(),
        created_by: meta.created_by.clone(),
        acceptance: meta.acceptance.clone(),
        links: meta.links.clone(),
        description: task.description.clone(),
        notes: task.notes.clone(),
        review: task.review.clone(),
        path,
    }
}

/// Runs a board write with the roster, then publishes the new journal entries as events.
fn write(
    engine: &mut Engine,
    op: impl FnOnce(&mut TaskBoard, &Roster) -> Result<Task, BoardError>,
) -> Result<TaskResult, ApiError> {
    let roster = roster(engine)?;
    let root = engine.project().to_owned();
    let state = state(engine)?;
    let task = op(&mut state.board, &roster).map_err(board_error)?;
    let task = view(&root, &state.board, &task);
    publish(engine)?;
    Ok(TaskResult { task })
}

/// Runs a board read (which may log hand edits), then publishes any new entries.
fn read<R>(
    engine: &mut Engine,
    op: impl FnOnce(&std::path::Path, &mut TaskBoard) -> Result<R, BoardError>,
) -> Result<R, ApiError> {
    roster(engine)?;
    let root = engine.project().to_owned();
    let result = op(&root, &mut state(engine)?.board).map_err(board_error)?;
    publish(engine)?;
    Ok(result)
}

impl Method for TaskCreate {
    const NAME: &'static str = "task.create";
    const MUTATES: bool = true;
    type Params = TaskCreateParams;
    type Result = TaskResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TaskCreateParams,
    ) -> Result<TaskResult, ApiError> {
        let actor = ctx.actor()?.clone();
        let uid = engine
            .ids_mut()
            .next_uuid()
            .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?;
        let new = NewTask {
            title: p.title,
            role: p.role,
            description: p.description.unwrap_or_default(),
            priority: p.priority.unwrap_or_default(),
            acceptance: p.acceptance.unwrap_or_default(),
            depends_on: p.depends_on.unwrap_or_default(),
            links: p.links.unwrap_or_default(),
        };
        write(engine, |board, roster| {
            board.create(new, uid.to_string(), &actor, roster)
        })
    }
}

impl Method for TaskGet {
    const NAME: &'static str = "task.get";
    const MUTATES: bool = false;
    type Params = TaskIdParams;
    type Result = TaskGetResult;
    fn call(engine: &mut Engine, _: &CallCtx, p: TaskIdParams) -> Result<TaskGetResult, ApiError> {
        read(engine, |root, board| {
            let task = board.get(&p.id)?;
            let history = board
                .history(&p.id)?
                .iter()
                .map(JournalEntry::to_json)
                .collect();
            Ok(TaskGetResult {
                task: view(root, board, &task),
                history,
            })
        })
    }
}

impl Method for TaskList {
    const NAME: &'static str = "task.list";
    const MUTATES: bool = false;
    type Params = TaskListParams;
    type Result = TaskListResult;
    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: TaskListParams,
    ) -> Result<TaskListResult, ApiError> {
        read(engine, |root, board| {
            let keep = |t: &Task| {
                p.status.is_none_or(|s| t.meta.status == s)
                    && p.role.as_ref().is_none_or(|r| &t.meta.role == r)
                    && p.assignee
                        .as_ref()
                        .is_none_or(|a| t.meta.assignee.as_ref() == Some(a))
            };
            let tasks = board.list()?;
            Ok(TaskListResult {
                tasks: tasks
                    .iter()
                    .filter(|t| keep(t))
                    .map(|t| view(root, board, t))
                    .collect(),
            })
        })
    }
}

impl Method for TaskClaim {
    const NAME: &'static str = "task.claim";
    const MUTATES: bool = true;
    type Params = TaskActorParams;
    type Result = TaskResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TaskActorParams,
    ) -> Result<TaskResult, ApiError> {
        let actor = ctx.actor()?.clone();
        write(engine, |board, _| board.claim(&p.id, &actor))
    }
}

impl Method for TaskUpdate {
    const NAME: &'static str = "task.update";
    const MUTATES: bool = true;
    type Params = TaskUpdateParams;
    type Result = TaskResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TaskUpdateParams,
    ) -> Result<TaskResult, ApiError> {
        let actor = ctx.actor()?.clone();
        let edit = TaskEdit {
            status: p.status,
            title: p.title,
            description: p.description,
            priority: p.priority,
            acceptance: p.acceptance,
            links: p.links,
            depends_on: p.depends_on,
            assignee: p.assignee,
        };
        write(engine, |board, _| board.update(&p.id, edit, &actor))
    }
}

impl Method for TaskComment {
    const NAME: &'static str = "task.comment";
    const MUTATES: bool = true;
    type Params = TaskCommentParams;
    type Result = TaskResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TaskCommentParams,
    ) -> Result<TaskResult, ApiError> {
        let actor = ctx.actor()?.clone();
        write(engine, |board, _| board.comment(&p.id, &p.text, &actor))
    }
}

impl Method for TaskApprove {
    const NAME: &'static str = "task.approve";
    const MUTATES: bool = true;
    type Params = TaskApproveParams;
    type Result = TaskResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TaskApproveParams,
    ) -> Result<TaskResult, ApiError> {
        let actor = ctx.actor()?.clone();
        let comment = p.comment.unwrap_or_default();
        write(engine, |board, roster| {
            board.review(&p.id, true, &comment, &actor, roster)
        })
    }
}

impl Method for TaskReject {
    const NAME: &'static str = "task.reject";
    const MUTATES: bool = true;
    type Params = TaskRejectParams;
    type Result = TaskResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TaskRejectParams,
    ) -> Result<TaskResult, ApiError> {
        let actor = ctx.actor()?.clone();
        write(engine, |board, roster| {
            board.review(&p.id, false, &p.comment, &actor, roster)
        })
    }
}

impl Method for StudioRoster {
    const NAME: &'static str = "studio.roster";
    const MUTATES: bool = false;
    type Params = NoParams;
    type Result = StudioRosterResult;
    fn call(engine: &mut Engine, _: &CallCtx, _: NoParams) -> Result<StudioRosterResult, ApiError> {
        let roster = roster(engine)?;
        let strings = |items: &[GateParty]| items.iter().map(ToString::to_string).collect();
        let roles = roster
            .roles()
            .iter()
            .map(|r| RoleView {
                role: r.role.to_string(),
                display_name: r.display_name.clone(),
                description: r.description.clone(),
                count: r.count.get(),
                model: r.model.clone(),
                mcp_tools: r.mcp_tools.clone(),
                harness_tools: r
                    .harness_tools
                    .iter()
                    .map(|h| format!("{h:?}").to_lowercase())
                    .collect(),
                bash_allow: r.bash_allow.clone(),
                write: r.write.clone(),
                read: r.read.clone(),
                review: strings(&r.review),
                skills: r.skills.clone(),
                max_turns: r.max_turns.map(std::num::NonZeroU32::get),
            })
            .collect();
        let disabled = roster
            .disabled()
            .iter()
            .map(|d| DisabledView {
                role: d.role.to_string(),
                reason: d.reason.clone(),
            })
            .collect();
        Ok(StudioRosterResult {
            team: roster.team.team.name.clone(),
            director: roster.director().to_string(),
            main_session_actor: roster.team.main_session_actor().to_string(),
            roles,
            disabled,
        })
    }
}

impl Method for StudioStatus {
    const NAME: &'static str = "studio.status";
    const MUTATES: bool = false;
    type Params = NoParams;
    type Result = StudioStatusResult;
    fn call(engine: &mut Engine, _: &CallCtx, _: NoParams) -> Result<StudioStatusResult, ApiError> {
        read(engine, |_, board| {
            let tasks = board.list()?;
            Ok(StudioStatusResult {
                tasks: counts(&tasks),
                pending_approvals: tasks
                    .iter()
                    .filter(|t| t.meta.status == TaskStatus::in_review)
                    .map(|t| t.meta.id.clone())
                    .collect(),
            })
        })
    }
}

impl Method for StudioActivity {
    const NAME: &'static str = "studio.activity";
    const MUTATES: bool = false;
    type Params = StudioActivityParams;
    type Result = StudioActivityResult;
    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: StudioActivityParams,
    ) -> Result<StudioActivityResult, ApiError> {
        let limit = p.limit.unwrap_or(100) as usize;
        read(engine, |_, board| {
            board.sync()?;
            let entries = board.journal().read(p.since)?;
            let picked: Vec<&JournalEntry> = entries
                .iter()
                .filter(|e| matches_filter(e, &p))
                .take(limit)
                .collect();
            let next = match picked.len() == limit {
                true => picked.last().map_or(0, |e| e.seq),
                false => entries.last().map_or(p.since.unwrap_or(0), |e| e.seq),
            };
            Ok(StudioActivityResult {
                entries: picked.into_iter().map(JournalEntry::to_json).collect(),
                next,
            })
        })
    }
}

fn matches_filter(entry: &JournalEntry, p: &StudioActivityParams) -> bool {
    let actor = &entry.actor;
    p.actor.as_ref().is_none_or(|a| actor.to_string() == *a)
        && p.role
            .as_ref()
            .is_none_or(|r| actor.kind() == ActorKind::Agent && actor.name() == r)
        && p.task
            .as_ref()
            .is_none_or(|t| entry.event.task() == Some(t.as_str()))
        && p.kind.as_ref().is_none_or(|k| entry.event.kind() == k)
}

/// Regenerate the Claude Code files (.claude/agents, .claude/skills, settings hooks, .mcp.json, the CLAUDE.md block) from studio/.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioSync;

/// `studio.sync` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StudioSyncParams {
    /// Report what would change and write nothing.
    pub check: Option<bool>,
    /// Overwrite generated files that were edited by hand.
    pub force: Option<bool>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// One file's outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SyncChangeView {
    /// Project-relative path.
    pub path: String,
    /// create, update, delete or unchanged.
    pub action: String,
    /// Unified diff (updates only).
    pub diff: Option<String>,
}

/// `studio.sync` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioSyncResult {
    /// Every file sync owns or merges into.
    pub changes: Vec<SyncChangeView>,
    /// Generated files edited by hand and left alone (pass force to overwrite).
    pub refused: Vec<String>,
    /// True if files are (check) or remain (write) out of date.
    pub stale: bool,
}

impl Method for StudioSync {
    const NAME: &'static str = "studio.sync";
    const MUTATES: bool = true;
    type Params = StudioSyncParams;
    type Result = StudioSyncResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: StudioSyncParams,
    ) -> Result<StudioSyncResult, ApiError> {
        let actor = ctx.actor()?.clone();
        let roster = roster(engine)?;
        let catalog = crate::scope::ToolCatalog::from_methods(engine.methods());
        let root = engine.project().to_owned();
        let options = SyncOptions {
            check: p.check.unwrap_or(false),
            force: p.force.unwrap_or(false),
        };
        let state = state(engine)?;
        let report = sync(&root, &state.dir, &roster, &catalog, options)
            .map_err(crate::module::config_error)?;
        let changed: Vec<String> = report
            .changes
            .iter()
            .filter(|c| c.action != SyncAction::Unchanged)
            .map(|c| c.path.clone())
            .collect();
        if !options.check && !changed.is_empty() {
            state
                .board
                .journal()
                .append(&actor, JournalEvent::Sync { changed })
                .map_err(crate::module::config_error)?;
        }
        publish(engine)?;
        Ok(StudioSyncResult {
            stale: match options.check {
                true => report.is_stale(),
                false => !report.refused.is_empty(),
            },
            changes: report
                .changes
                .into_iter()
                .map(|c| SyncChangeView {
                    path: c.path,
                    action: format!("{:?}", c.action).to_lowercase(),
                    diff: c.diff,
                })
                .collect(),
            refused: report.refused,
        })
    }
}
