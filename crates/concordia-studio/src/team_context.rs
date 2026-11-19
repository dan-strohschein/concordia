//! Team questions as hook context (ADR-0081): what a role, the director or an interactive
//! director should hear about, each delivered once. Cursors live in `.concordia/studio/cursors.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::journal::StudioJournal;
use crate::task::{Question, QuestionStatus, fold};

/// What the hooks remember having delivered: key → journal `seq`.
const CURSORS: &str = ".concordia/studio/cursors.json";

/// For a subagent starting as `role`: open questions addressed to it, and answers to its role's
/// questions not delivered before.
pub(crate) fn for_role(root: &Path, role: &str) -> Result<String, String> {
    let (questions, last) = questions(root)?;
    let key = format!("start:{role}");
    let seen = cursor(root, &key);
    let open: Vec<String> = questions
        .iter()
        .filter(|q| q.status == QuestionStatus::open && q.to == role)
        .map(|q| format!("- {} from {}: {}{}", q.id, q.from, q.text, about(q)))
        .collect();
    let answers: Vec<String> = questions
        .iter()
        .filter(|q| asked_by_role(q, role) && q.last_seq > seen.unwrap_or(0))
        .filter_map(answer_line)
        .collect();
    set_cursor(root, &key, last)?;
    let mut parts = Vec::new();
    if !open.is_empty() {
        parts.push(format!(
            "Questions for your role (answer with mcp__concordia__studio_answer if you can; keep it short):\n{}",
            open.join("\n")
        ));
    }
    if !answers.is_empty() {
        parts.push(format!(
            "Answers to your role's questions:\n{}",
            answers.join("\n")
        ));
    }
    Ok(parts.join("\n"))
}

/// For the director after a subagent returns: questions opened, answered or escalated since its
/// last turn in `session` (the first turn: every unanswered question).
pub(crate) fn for_director(root: &Path, session: &str) -> Result<Option<String>, String> {
    let (questions, last) = questions(root)?;
    let key = format!("turn:{session}");
    let seen = cursor(root, &key);
    set_cursor(root, &key, last)?;
    let lines: Vec<String> = questions
        .iter()
        .filter(|q| match seen {
            Some(seen) => q.last_seq > seen,
            None => q.status != QuestionStatus::answered,
        })
        .map(director_line)
        .collect();
    Ok((!lines.is_empty()).then(|| {
        format!(
            "Team questions since your last turn:\n{}\nRoute open ones: answer with mcp__concordia__studio_answer, delegate to the addressed role, or, if no role can answer, mcp__concordia__studio_escalate to the human. Re-run askers that are blocked on an answered question.",
            lines.join("\n")
        )
    }))
}

/// For an interactive director's next prompt: human answers since its last prompt in `session`.
pub(crate) fn human_answers(root: &Path, session: &str) -> Result<Option<String>, String> {
    let (questions, last) = questions(root)?;
    let key = format!("prompt:{session}");
    let seen = cursor(root, &key);
    set_cursor(root, &key, last)?;
    let Some(seen) = seen else {
        return Ok(None);
    };
    let lines: Vec<String> = questions
        .iter()
        .filter(|q| q.last_seq > seen)
        .filter(|q| {
            q.answer
                .as_ref()
                .is_some_and(|a| a.by.starts_with("human:"))
        })
        .filter_map(answer_line)
        .collect();
    Ok((!lines.is_empty()).then(|| {
        format!(
            "The human answered:\n{}\nUnblocked tasks are back in todo; re-run their roles.",
            lines.join("\n")
        )
    }))
}

fn questions(root: &Path) -> Result<(Vec<Question>, u64), String> {
    let journal = StudioJournal::of_project(root);
    let entries = journal.read(None).map_err(|e| e.to_string())?;
    let last = entries.last().map_or(0, |e| e.seq);
    Ok((fold(&entries), last))
}

fn asked_by_role(q: &Question, role: &str) -> bool {
    q.from
        .parse::<concordia_command::Actor>()
        .is_ok_and(|a| a.name() == role)
}

fn about(q: &Question) -> String {
    q.task
        .as_deref()
        .map(|t| format!(" (task {t})"))
        .unwrap_or_default()
}

fn answer_line(q: &Question) -> Option<String> {
    let a = q.answer.as_ref()?;
    Some(format!("- {} \"{}\" → {}: {}", q.id, q.text, a.by, a.text))
}

fn director_line(q: &Question) -> String {
    match (&q.status, &q.answer, &q.escalation) {
        (QuestionStatus::answered, Some(a), _) => {
            format!("- {} answered by {}: {}", q.id, a.by, a.text)
        }
        (QuestionStatus::escalated, _, Some(e)) => format!(
            "- {} escalated by {} ({}); waiting for the human",
            q.id, e.by, e.reason
        ),
        _ => format!("- {} {} → {}: {}{}", q.id, q.from, q.to, q.text, about(q)),
    }
}

fn cursors_path(root: &Path) -> PathBuf {
    root.join(CURSORS)
}

fn read_cursors(root: &Path) -> BTreeMap<String, u64> {
    std::fs::read_to_string(cursors_path(root))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn cursor(root: &Path, key: &str) -> Option<u64> {
    read_cursors(root).get(key).copied()
}

fn set_cursor(root: &Path, key: &str, seq: u64) -> Result<(), String> {
    let mut cursors = read_cursors(root);
    cursors.insert(key.to_owned(), seq);
    let path = cursors_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_vec(&cursors).map_err(|e| e.to_string())?;
    concordia_core::fs::write_atomically(&path, &text, concordia_core::fs::Visibility::Default)
        .map_err(|e| e.to_string())
}
