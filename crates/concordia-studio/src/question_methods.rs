//! `studio.ask`, `studio.answer`, `studio.escalate` and `studio.questions` (ADR-0081).

use concordia_api::{ApiError, CallCtx, Engine, Method, MethodInfo};
use concordia_command::Actor;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::module::{board_error, publish, roster, state};
use crate::roster::Roster;
use crate::task::{BoardError, NewQuestion, Question, QuestionStatus, TaskBoard};

/// The question methods.
pub(crate) fn all() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<StudioAsk>(),
        MethodInfo::of::<StudioAnswer>(),
        MethodInfo::of::<StudioEscalate>(),
        MethodInfo::of::<StudioQuestions>(),
    ]
}

/// Ask a teammate a question (telegraphic, ≤ 280 chars; refer to ids, don't restate context). Returns at once with an id; don't wait for the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioAsk;

/// `studio.ask` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StudioAskParams {
    /// An enabled role id (e.g. `qa-playtester`, `director`) or `human`.
    pub to: String,
    /// The question, at most 280 characters.
    pub text: String,
    /// The task it is about.
    pub task: Option<String>,
    /// Related ids (tasks, questions).
    pub refs: Option<Vec<String>>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Answer a question addressed to your role (the director and humans may answer any; escalated ones take only a human answer). ≤ 500 chars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioAnswer;

/// `studio.answer` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StudioAnswerParams {
    /// Question id (`Q-0001`).
    pub id: String,
    /// The answer, at most 500 characters.
    pub text: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Escalate a question nobody on the team can answer to the human (director only); its unfinished task becomes blocked until a human answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioEscalate;

/// `studio.escalate` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StudioEscalateParams {
    /// Question id.
    pub id: String,
    /// Why no role can answer, at most 280 characters.
    pub reason: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// List team questions, oldest first: who asked what and when, who answered what and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioQuestions;

/// `studio.questions` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StudioQuestionsParams {
    /// Only questions in this state.
    pub status: Option<QuestionStatus>,
    /// Only questions to this role (or `human`).
    pub to: Option<String>,
    /// Only questions from this role or actor.
    pub from: Option<String>,
    /// Only questions about this task.
    pub task: Option<String>,
    /// Only questions with an event after this journal `seq` (a previous `next`).
    pub since: Option<u64>,
    /// At most this many.
    pub limit: Option<u32>,
}

/// A question result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct QuestionResult {
    /// The question.
    pub question: Question,
}

/// `studio.questions` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StudioQuestionsResult {
    /// Matching questions, in id order.
    pub questions: Vec<Question>,
    /// Pass as `since` to get only newer changes.
    pub next: u64,
}

/// Runs a desk operation with the roster, then publishes the new journal entries.
fn desk(
    engine: &mut Engine,
    op: impl FnOnce(&mut TaskBoard, &Roster) -> Result<Question, BoardError>,
) -> Result<QuestionResult, ApiError> {
    let roster = roster(engine)?;
    let question = op(&mut state(engine)?.board, &roster).map_err(board_error)?;
    publish(engine)?;
    Ok(QuestionResult { question })
}

impl Method for StudioAsk {
    const NAME: &'static str = "studio.ask";
    const MUTATES: bool = true;
    type Params = StudioAskParams;
    type Result = QuestionResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: StudioAskParams,
    ) -> Result<QuestionResult, ApiError> {
        let actor = ctx.actor()?.clone();
        let new = NewQuestion {
            to: p.to,
            text: p.text,
            task: p.task,
            refs: p.refs.unwrap_or_default(),
        };
        desk(engine, |board, roster| board.ask(new, &actor, roster))
    }
}

impl Method for StudioAnswer {
    const NAME: &'static str = "studio.answer";
    const MUTATES: bool = true;
    type Params = StudioAnswerParams;
    type Result = QuestionResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: StudioAnswerParams,
    ) -> Result<QuestionResult, ApiError> {
        let actor = ctx.actor()?.clone();
        desk(engine, |board, roster| {
            board.answer(&p.id, &p.text, &actor, roster)
        })
    }
}

impl Method for StudioEscalate {
    const NAME: &'static str = "studio.escalate";
    const MUTATES: bool = true;
    type Params = StudioEscalateParams;
    type Result = QuestionResult;
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: StudioEscalateParams,
    ) -> Result<QuestionResult, ApiError> {
        let actor = ctx.actor()?.clone();
        desk(engine, |board, roster| {
            board.escalate(&p.id, &p.reason, &actor, roster)
        })
    }
}

impl Method for StudioQuestions {
    const NAME: &'static str = "studio.questions";
    const MUTATES: bool = false;
    type Params = StudioQuestionsParams;
    type Result = StudioQuestionsResult;
    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: StudioQuestionsParams,
    ) -> Result<StudioQuestionsResult, ApiError> {
        roster(engine)?;
        let (all, next) = state(engine)?.board.questions().map_err(board_error)?;
        publish(engine)?;
        let limit = p.limit.map_or(usize::MAX, |l| l as usize);
        let questions = all
            .into_iter()
            .filter(|q| matches(q, &p))
            .take(limit)
            .collect();
        Ok(StudioQuestionsResult { questions, next })
    }
}

fn matches(q: &Question, p: &StudioQuestionsParams) -> bool {
    p.status.is_none_or(|s| q.status == s)
        && p.to.as_ref().is_none_or(|to| &q.to == to)
        && p.from
            .as_ref()
            .is_none_or(|from| from_matches(&q.from, from))
        && p.task.as_ref().is_none_or(|t| q.task.as_ref() == Some(t))
        && p.since.is_none_or(|since| q.last_seq > since)
}

/// `from` is a full actor (`agent:qa-playtester#1`) or a role (`qa-playtester`).
fn from_matches(asker: &str, from: &str) -> bool {
    asker == from || asker.parse::<Actor>().is_ok_and(|a| a.name() == from)
}
