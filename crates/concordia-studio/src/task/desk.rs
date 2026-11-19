//! The question desk (ADR-0081): asking, answering and escalating questions on the task board.
//! Questions are journal entries only; ids are allocated under the board lock.

use concordia_command::Actor;
use concordia_core::suggest::did_you_mean;

use super::board::{BoardError, TaskBoard};
use super::model::TaskStatus;
use super::question::{
    self, ANSWER_LIMIT, ASK_LIMIT, HUMAN, Question, QuestionStatus, REASON_LIMIT,
};
use super::store::BoardLock;
use crate::journal::JournalEvent;
use crate::roster::Roster;

/// Who escalations are attributed to when they block a task.
const SYSTEM: &str = "system:studio";

/// A new question.
#[derive(Debug, Clone, Default)]
pub struct NewQuestion {
    /// A role id or `human`.
    pub to: String,
    /// The question.
    pub text: String,
    /// The task it is about.
    pub task: Option<String>,
    /// Related ids.
    pub refs: Vec<String>,
}

impl TaskBoard {
    /// Records a question to an enabled role or `human`; ids are sequential across processes.
    pub fn ask(
        &mut self,
        new: NewQuestion,
        actor: &Actor,
        roster: &Roster,
    ) -> Result<Question, BoardError> {
        check_addressee(&new.to, roster)?;
        let text = question::checked_text(&new.text, ASK_LIMIT, "a question")?;
        let lock = self.store.lock()?;
        if let Some(task) = &new.task {
            self.task(task)?;
        }
        let id = question::next_id(&self.all_questions()?);
        let event = JournalEvent::QuestionAsked {
            question: id.clone(),
            to: new.to,
            text,
            task: new.task,
            refs: new.refs,
        };
        self.store.record(&lock, actor, event)?;
        self.question(&id)
    }

    /// Answers a question; a human answer to an escalated question unblocks its task.
    pub fn answer(
        &mut self,
        id: &str,
        text: &str,
        actor: &Actor,
        roster: &Roster,
    ) -> Result<Question, BoardError> {
        let text = question::checked_text(text, ANSWER_LIMIT, "an answer")?;
        let lock = self.store.lock()?;
        let q = self.question(id)?;
        question::check_answer(&q, actor, roster.director().as_str())?;
        let event = JournalEvent::QuestionAnswered {
            question: id.to_owned(),
            text,
        };
        self.store.record(&lock, actor, event)?;
        if q.status == QuestionStatus::escalated {
            self.set_task_status(
                &lock,
                q.task.as_deref(),
                TaskStatus::blocked,
                TaskStatus::todo,
                actor,
            )?;
        }
        self.question(id)
    }

    /// Re-addresses an open question to the human (director only) and blocks its unfinished task.
    pub fn escalate(
        &mut self,
        id: &str,
        reason: &str,
        actor: &Actor,
        roster: &Roster,
    ) -> Result<Question, BoardError> {
        let reason = question::checked_text(reason, REASON_LIMIT, "an escalation reason")?;
        let lock = self.store.lock()?;
        let q = self.question(id)?;
        question::check_escalate(&q, actor, roster.director().as_str())?;
        let event = JournalEvent::QuestionEscalated {
            question: id.to_owned(),
            reason,
        };
        self.store.record(&lock, actor, event)?;
        let system: Actor = SYSTEM.parse().expect("valid literal actor");
        if let Some(task) = q.task.as_deref() {
            let status = self.task(task)?.meta.status;
            if !matches!(status, TaskStatus::done | TaskStatus::blocked) {
                self.set_task_status(&lock, Some(task), status, TaskStatus::blocked, &system)?;
            }
        }
        self.question(id)
    }

    /// Every question, in id order, plus the journal's last `seq` (pass it as `since`).
    pub fn questions(&mut self) -> Result<(Vec<Question>, u64), BoardError> {
        let _lock = self.store.lock()?;
        let questions = self.all_questions()?;
        let last = self.journal().last_seq()?;
        Ok((questions, last))
    }

    fn all_questions(&self) -> Result<Vec<Question>, BoardError> {
        Ok(question::fold(&self.journal().read(None)?))
    }

    fn question(&self, id: &str) -> Result<Question, BoardError> {
        self.all_questions()?
            .into_iter()
            .find(|q| q.id == id)
            .ok_or_else(|| BoardError::Invalid {
                message: format!("no question {id}"),
                suggestions: vec!["list questions with studio.questions".into()],
            })
    }

    /// Moves `task` from `from` to `to` (no-op without a task or in another status).
    fn set_task_status(
        &mut self,
        lock: &BoardLock,
        task: Option<&str>,
        from: TaskStatus,
        to: TaskStatus,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let Some(task) = task else {
            return Ok(());
        };
        if self.task(task)?.meta.status != from {
            return Ok(());
        }
        self.change_locked(lock, task, actor, |t| {
            t.meta.status = to;
            Ok(())
        })
        .map(drop)
    }
}

/// `to` must be an enabled role or `human`.
fn check_addressee(to: &str, roster: &Roster) -> Result<(), BoardError> {
    if to == HUMAN || roster.role(to).is_some() {
        return Ok(());
    }
    let names: Vec<&str> = roster
        .roles()
        .iter()
        .map(|r| r.role.as_str())
        .chain([HUMAN])
        .collect();
    let hint = did_you_mean(to, names.iter().copied())
        .first()
        .map(|s| format!("; did you mean {s}?"))
        .unwrap_or_default();
    Err(BoardError::Invalid {
        message: format!("{to:?} is not an enabled role or \"human\"{hint}"),
        suggestions: vec!["see studio.roster for the enabled roles".into()],
    })
}
