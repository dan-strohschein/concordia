//! `txn.begin`, `txn.commit`, `txn.abort`, `txn.undo`, `txn.undo_to`, `txn.redo`, `txn.history`.

use concordia_command::{Actor, Author, EntryKind};
use concordia_core::id::{EntityId, TxnId};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use super::{conflict, txn_id};
use crate::engine::{Engine, TxnHold};
use crate::error::{ApiError, ApiErrorKind};
use crate::map::{self, Hints};
use crate::method::{CallCtx, Method, MethodInfo};

/// Start a transaction: later mutating calls from this session join it until txn.commit or txn.abort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnBegin;

/// `txn.begin` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TxnBeginParams {
    /// What the transaction does, e.g. "build arena".
    pub label: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `txn.begin` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnBeginResult {
    /// Pass this to txn.commit or txn.abort.
    pub txn: TxnId,
}

/// Commit this session's open transaction (one undo step).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnCommit;

/// Roll back and close this session's open transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnAbort;

/// `txn.commit` / `txn.abort` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TxnRefParams {
    /// The id from txn.begin.
    pub txn: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `txn.commit` / `txn.abort` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct OpsResult {
    /// Number of ops in the transaction.
    pub ops: u64,
}

/// Undo the latest committed transaction (any session's), attributed to the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnUndo;

/// Undo every transaction back to and including `txn` (from txn.history), each attributed to the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnUndoTo;

/// `txn.undo_to` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TxnUndoToParams {
    /// The oldest transaction to undo (a commit's `txn` from txn.history).
    pub txn: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `txn.undo_to` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct UndoToResult {
    /// The transactions undone, latest first (txn.redo replays them oldest first).
    pub undone: Vec<TxnId>,
    /// Who undid them.
    pub by: Actor,
}

/// Redo the latest undone transaction, attributed to the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnRedo;

/// `txn.undo` / `txn.redo` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ActorOnly {
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `txn.undo` / `txn.redo` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct UndoResult {
    /// The transaction undone or redone.
    pub txn: TxnId,
    /// Its label.
    pub label: String,
    /// Who undid or redid it.
    pub by: Actor,
}

/// List recent history events (commit, undo, redo, abort, save), oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnHistory;

/// `txn.history` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TxnHistoryParams {
    /// Maximum entries (default 50).
    #[reflect(range(min = 1.0, max = 10000.0))]
    pub limit: Option<u32>,
}

/// One history event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct HistoryRow {
    /// Sequence number.
    pub seq: u64,
    /// "commit", "undo", "redo", "abort" or "save".
    pub kind: String,
    /// The transaction (commit, abort).
    pub txn: Option<TxnId>,
    /// The target transaction (undo, redo).
    pub target: Option<TxnId>,
    /// Who.
    pub actor: Actor,
    /// Transaction label (or saved path).
    pub label: String,
    /// Op count (commit).
    pub ops: u64,
    /// Abort reason.
    pub reason: Option<String>,
    /// When, in milliseconds since the Unix epoch.
    pub time_ms: u64,
    /// Entities the transaction touched (commit, undo, redo).
    pub entities: Vec<EntityId>,
    /// For a commit: whether it is currently undone.
    pub undone: bool,
}

/// `txn.history` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TxnHistoryResult {
    /// Events, oldest first.
    pub entries: Vec<HistoryRow>,
}

impl Method for TxnBegin {
    const NAME: &'static str = "txn.begin";
    const MUTATES: bool = true;
    type Params = TxnBeginParams;
    type Result = TxnBeginResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TxnBeginParams,
    ) -> Result<TxnBeginResult, ApiError> {
        if let Some(err) = conflict(engine.scene()?, ctx) {
            return Err(err);
        }
        let author = author(ctx)?;
        let Engine {
            scene,
            registry,
            ids,
            ..
        } = engine;
        let scene = scene
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))?;
        let txn = scene
            .log
            .begin(&scene.world, author, p.label, ids)
            .map_err(|e| {
                map::command(
                    e,
                    None,
                    &Hints {
                        registry,
                        world: None,
                    },
                )
            })?;
        scene.hold = Some(TxnHold {
            session: ctx.session,
            txn,
            since: ctx.now,
        });
        Ok(TxnBeginResult { txn })
    }
}

/// Checks that `given` is the caller's open transaction.
fn own_txn(engine: &Engine, ctx: &CallCtx, given: &str) -> Result<(), ApiError> {
    let scene = engine.scene()?;
    let txn = txn_id(given)?;
    match scene.hold {
        Some(hold) if hold.session != ctx.session => {
            Err(conflict(scene, ctx)
                .unwrap_or_else(|| ApiError::from_kind(ApiErrorKind::TxnConflict)))
        }
        Some(hold) if hold.txn == txn => Ok(()),
        _ => Err(ApiError::new(
            ApiErrorKind::TxnNotFound,
            format!("transaction {given} is not open (it may have timed out)"),
        )
        .at("/txn")),
    }
}

impl Method for TxnCommit {
    const NAME: &'static str = "txn.commit";
    const MUTATES: bool = true;
    type Params = TxnRefParams;
    type Result = OpsResult;

    fn call(engine: &mut Engine, ctx: &CallCtx, p: TxnRefParams) -> Result<OpsResult, ApiError> {
        own_txn(engine, ctx, &p.txn)?;
        let Engine {
            scene, registry, ..
        } = engine;
        let scene = scene
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))?;
        let ops = scene.log.open_txn().map_or(0, |t| t.op_count() as u64);
        scene.hold = None;
        scene.log.commit(&mut scene.world, registry).map_err(|e| {
            map::command(
                e,
                None,
                &Hints {
                    registry,
                    world: None,
                },
            )
        })?;
        scene.dirty |= ops > 0;
        Ok(OpsResult { ops })
    }
}

impl Method for TxnAbort {
    const NAME: &'static str = "txn.abort";
    const MUTATES: bool = true;
    type Params = TxnRefParams;
    type Result = OpsResult;

    fn call(engine: &mut Engine, ctx: &CallCtx, p: TxnRefParams) -> Result<OpsResult, ApiError> {
        own_txn(engine, ctx, &p.txn)?;
        let Engine {
            scene, registry, ..
        } = engine;
        let scene = scene
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))?;
        let ops = scene.log.open_txn().map_or(0, |t| t.op_count() as u64);
        scene.hold = None;
        scene
            .log
            .abort(&mut scene.world, registry, "aborted")
            .map_err(|e| {
                map::command(
                    e,
                    None,
                    &Hints {
                        registry,
                        world: None,
                    },
                )
            })?;
        Ok(OpsResult { ops })
    }
}

impl Method for TxnUndo {
    const NAME: &'static str = "txn.undo";
    const MUTATES: bool = true;
    type Params = ActorOnly;
    type Result = UndoResult;

    fn call(engine: &mut Engine, ctx: &CallCtx, _: ActorOnly) -> Result<UndoResult, ApiError> {
        step(engine, ctx, true)
    }
}

impl Method for TxnUndoTo {
    const NAME: &'static str = "txn.undo_to";
    const MUTATES: bool = true;
    type Params = TxnUndoToParams;
    type Result = UndoToResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: TxnUndoToParams,
    ) -> Result<UndoToResult, ApiError> {
        let target = txn_id(&p.txn)?;
        if let Some(err) = conflict(engine.scene()?, ctx) {
            return Err(err);
        }
        let author = author(ctx)?;
        let by = author.actor.clone();
        let Engine {
            scene, registry, ..
        } = engine;
        let scene = scene
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))?;
        let result = scene
            .log
            .undo_to(&mut scene.world, registry, &author, target);
        // Undos that succeeded before a failure stay undone, so the scene is dirty either way.
        scene.dirty = true;
        let undone = result.map_err(|e| {
            map::command(
                e,
                None,
                &Hints {
                    registry,
                    world: Some(&scene.world),
                },
            )
            .at("/txn")
        })?;
        Ok(UndoToResult { undone, by })
    }
}

impl Method for TxnRedo {
    const NAME: &'static str = "txn.redo";
    const MUTATES: bool = true;
    type Params = ActorOnly;
    type Result = UndoResult;

    fn call(engine: &mut Engine, ctx: &CallCtx, _: ActorOnly) -> Result<UndoResult, ApiError> {
        step(engine, ctx, false)
    }
}

/// Undo (`back`) or redo, reporting the transaction's label.
fn step(engine: &mut Engine, ctx: &CallCtx, back: bool) -> Result<UndoResult, ApiError> {
    if let Some(err) = conflict(engine.scene()?, ctx) {
        return Err(err);
    }
    let author = author(ctx)?;
    let by = author.actor.clone();
    let Engine {
        scene, registry, ..
    } = engine;
    let scene = scene
        .as_mut()
        .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))?;
    let result = match back {
        true => scene.log.undo(&mut scene.world, registry, author),
        false => scene.log.redo(&mut scene.world, registry, author),
    };
    let txn = result.map_err(|e| {
        map::command(
            e,
            None,
            &Hints {
                registry,
                world: Some(&scene.world),
            },
        )
    })?;
    let label = scene
        .log
        .history(usize::MAX)
        .iter()
        .find(|e| e.txn == Some(txn))
        .map(|e| e.label.clone())
        .unwrap_or_default();
    scene.dirty = true;
    Ok(UndoResult { txn, label, by })
}

impl Method for TxnHistory {
    const NAME: &'static str = "txn.history";
    const MUTATES: bool = false;
    type Params = TxnHistoryParams;
    type Result = TxnHistoryResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: TxnHistoryParams,
    ) -> Result<TxnHistoryResult, ApiError> {
        let log = &engine.scene()?.log;
        let is_undone = |txn: Option<TxnId>| log.undone().iter().any(|t| Some(t.id) == txn);
        let entries = log
            .history(p.limit.unwrap_or(50) as usize)
            .into_iter()
            .map(|e| HistoryRow {
                undone: e.kind == EntryKind::Commit && is_undone(e.txn),
                time_ms: e.ts_ms,
                entities: e.entities,
                seq: e.seq,
                kind: kind(e.kind).to_owned(),
                txn: e.txn,
                target: e.target,
                actor: e.actor,
                label: e.label,
                ops: e.ops as u64,
                reason: e.reason,
            })
            .collect();
        Ok(TxnHistoryResult { entries })
    }
}

fn kind(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Commit => "commit",
        EntryKind::Undo => "undo",
        EntryKind::Redo => "redo",
        EntryKind::Abort => "abort",
        EntryKind::Save => "save",
    }
}

fn author(ctx: &CallCtx) -> Result<Author, ApiError> {
    Ok(Author {
        actor: ctx.actor()?.clone(),
        session: Some(ctx.session),
    })
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<TxnBegin>(),
        MethodInfo::of::<TxnCommit>(),
        MethodInfo::of::<TxnAbort>(),
        MethodInfo::of::<TxnUndo>(),
        MethodInfo::of::<TxnUndoTo>(),
        MethodInfo::of::<TxnRedo>(),
        MethodInfo::of::<TxnHistory>(),
    ]
}
