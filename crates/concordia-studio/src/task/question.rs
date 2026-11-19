//! Team questions (ADR-0081): state derived from the Studio journal, and who may do what.

use concordia_command::{Actor, ActorKind};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use super::board::BoardError;
use crate::journal::{JournalEntry, JournalEvent};

/// The addressee for questions only a person can answer.
pub const HUMAN: &str = "human";
/// Longest question, in characters.
pub const ASK_LIMIT: usize = 280;
/// Longest answer, in characters.
pub const ANSWER_LIMIT: usize = 500;
/// Longest escalation reason, in characters.
pub const REASON_LIMIT: usize = 280;

/// Where a question stands.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum QuestionStatus {
    /// Waiting for an answer from its addressee.
    open,
    /// Answered; closed.
    answered,
    /// Escalated to the human; waiting for a human answer.
    escalated,
}

/// An answer: who, what, when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct QuestionAnswer {
    /// Who answered.
    pub by: String,
    /// The answer.
    pub text: String,
    /// Unix milliseconds.
    pub ms: u64,
}

/// An escalation: who, why, when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct QuestionEscalation {
    /// Who escalated (the director).
    pub by: String,
    /// Why no one on the team can answer.
    pub reason: String,
    /// Unix milliseconds.
    pub ms: u64,
}

/// A question as the API returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Question {
    /// Id (`Q-0001`).
    pub id: String,
    /// Who asked.
    pub from: String,
    /// Addressee: a role id or `human` (after escalation, `human`).
    pub to: String,
    /// The task it is about.
    pub task: Option<String>,
    /// Related ids.
    pub refs: Vec<String>,
    /// The question.
    pub text: String,
    /// When it was asked (Unix milliseconds).
    pub asked_ms: u64,
    /// Where it stands.
    pub status: QuestionStatus,
    /// The answer, once there is one.
    pub answer: Option<QuestionAnswer>,
    /// The escalation, if any.
    pub escalation: Option<QuestionEscalation>,
    /// The journal `seq` of its latest event (for `since`).
    #[serde(skip)]
    pub last_seq: u64,
}

/// Every question in the journal, in id order.
pub fn fold(entries: &[JournalEntry]) -> Vec<Question> {
    let mut questions: Vec<Question> = Vec::new();
    for entry in entries {
        apply(&mut questions, entry);
    }
    questions
}

fn apply(questions: &mut Vec<Question>, entry: &JournalEntry) {
    let actor = entry.actor.to_string();
    match &entry.event {
        JournalEvent::QuestionAsked {
            question,
            to,
            text,
            task,
            refs,
        } => questions.push(Question {
            id: question.clone(),
            from: actor,
            to: to.clone(),
            task: task.clone(),
            refs: refs.clone(),
            text: text.clone(),
            asked_ms: entry.ts_ms,
            status: QuestionStatus::open,
            answer: None,
            escalation: None,
            last_seq: entry.seq,
        }),
        JournalEvent::QuestionAnswered { question, text } => {
            if let Some(q) = find_mut(questions, question) {
                q.status = QuestionStatus::answered;
                q.answer = Some(QuestionAnswer {
                    by: actor,
                    text: text.clone(),
                    ms: entry.ts_ms,
                });
                q.last_seq = entry.seq;
            }
        }
        JournalEvent::QuestionEscalated { question, reason } => {
            if let Some(q) = find_mut(questions, question) {
                q.status = QuestionStatus::escalated;
                q.to = HUMAN.to_owned();
                q.escalation = Some(QuestionEscalation {
                    by: actor,
                    reason: reason.clone(),
                    ms: entry.ts_ms,
                });
                q.last_seq = entry.seq;
            }
        }
        _ => {}
    }
}

fn find_mut<'a>(questions: &'a mut [Question], id: &str) -> Option<&'a mut Question> {
    questions.iter_mut().find(|q| q.id == id)
}

/// The id after the last question (`Q-0001` first).
pub fn next_id(questions: &[Question]) -> String {
    format!("Q-{:04}", questions.len() + 1)
}

/// `text` trimmed, non-empty and at most `limit` characters.
pub fn checked_text(text: &str, limit: usize, what: &str) -> Result<String, BoardError> {
    let text = text.trim();
    let count = text.chars().count();
    if text.is_empty() {
        return Err(invalid(format!("{what} needs text")));
    }
    if count > limit {
        return Err(invalid(format!(
            "{what} is {count} characters; the limit is {limit}. Be telegraphic and refer to ids instead of restating context (concordia-team-comms)"
        )));
    }
    Ok(text.to_owned())
}

/// True if `actor` is an instance of the director role.
pub fn is_director(actor: &Actor, director: &str) -> bool {
    actor.kind() == ActorKind::Agent && actor.name() == director
}

/// An unanswered question may be answered by its addressee's role, the director or a human;
/// an escalated one only by a human.
pub fn check_answer(q: &Question, actor: &Actor, director: &str) -> Result<(), BoardError> {
    match q.status {
        QuestionStatus::answered => {
            return Err(invalid(format!(
                "{} is already answered; a question closes on its first answer",
                q.id
            )));
        }
        QuestionStatus::escalated if !actor.is_human() => {
            return Err(BoardError::NotPermitted(format!(
                "{} was escalated; only a human may answer it, not {actor}",
                q.id
            )));
        }
        _ => {}
    }
    let addressee = actor.kind() == ActorKind::Agent && actor.name() == q.to;
    if actor.is_human() || addressee || is_director(actor, director) {
        return Ok(());
    }
    Err(BoardError::NotPermitted(format!(
        "{} is addressed to {}; only {}, the director or a human may answer it, not {actor}",
        q.id, q.to, q.to
    )))
}

/// Only the director escalates, and only an open question.
pub fn check_escalate(q: &Question, actor: &Actor, director: &str) -> Result<(), BoardError> {
    if !is_director(actor, director) {
        return Err(BoardError::NotPermitted(format!(
            "only the director escalates questions, not {actor}"
        )));
    }
    match q.status {
        QuestionStatus::open => Ok(()),
        QuestionStatus::answered => Err(invalid(format!("{} is already answered", q.id))),
        QuestionStatus::escalated => Err(invalid(format!("{} is already escalated", q.id))),
    }
}

fn invalid(message: String) -> BoardError {
    BoardError::Invalid {
        message,
        suggestions: vec!["see studio.questions for each question's state".into()],
    }
}
