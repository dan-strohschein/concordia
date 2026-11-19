//! Role instance slots: `agent:<role>#n`, shared by `concordia mcp` (which claims them) and the
//! SubagentStop hook (which releases them), in `.concordia/studio/slots.json` under a file lock.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::StudioError;

/// Who holds a slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Holder {
    /// `<session_id>/<agent_id>` from the hook.
    agent: String,
    /// The `concordia mcp` process that claimed it (dead holders are reclaimed).
    pid: u32,
}

/// role → slot number → holder.
type Table = BTreeMap<String, BTreeMap<u32, Holder>>;

/// The project's slot registry.
#[derive(Debug, Clone)]
pub struct Slots {
    path: PathBuf,
}

impl Slots {
    /// The registry of the project at `root`.
    pub fn of_project(root: &Path) -> Self {
        Self {
            path: root.join(".concordia/studio/slots.json"),
        }
    }

    /// The slot `agent` holds in `role`, else the lowest free one up to `count`, held for `pid`.
    pub fn claim(
        &self,
        role: &str,
        agent: &str,
        count: u32,
        pid: u32,
    ) -> Result<NonZeroU32, StudioError> {
        self.update(|table| {
            let slots = table.entry(role.to_owned()).or_default();
            slots.retain(|_, h| concordia_core::process::alive(h.pid));
            if let Some((n, _)) = slots.iter().find(|(_, h)| h.agent == agent) {
                return NonZeroU32::new(*n).ok_or_else(|| StudioError::invalid("slot 0"));
            }
            let free = (1..=count).find(|n| !slots.contains_key(n)).ok_or_else(|| {
                StudioError::invalid(format!(
                    "role {role} is at capacity ({count} instance(s) busy); wait for one to finish or raise its count in studio/team.toml"
                ))
            })?;
            slots.insert(
                free,
                Holder {
                    agent: agent.to_owned(),
                    pid,
                },
            );
            NonZeroU32::new(free).ok_or_else(|| StudioError::invalid("slot 0"))
        })
    }

    /// Frees every slot `agent` holds.
    pub fn release(&self, agent: &str) -> Result<(), StudioError> {
        self.update(|table| {
            table
                .values_mut()
                .for_each(|slots| slots.retain(|_, h| h.agent != agent));
            Ok(())
        })
    }

    /// Frees every slot held by agents of `session` (keys `<session>/...`).
    pub fn release_session(&self, session: &str) -> Result<(), StudioError> {
        let prefix = format!("{session}/");
        self.update(|table| {
            table
                .values_mut()
                .for_each(|slots| slots.retain(|_, h| !h.agent.starts_with(&prefix)));
            Ok(())
        })
    }

    /// The (role, slot) `agent` holds, if any.
    pub fn lookup(&self, agent: &str) -> Result<Option<(String, NonZeroU32)>, StudioError> {
        let table = self.read_table()?;
        Ok(table.into_iter().find_map(|(role, slots)| {
            slots
                .into_iter()
                .find(|(_, h)| h.agent == agent)
                .and_then(|(n, _)| NonZeroU32::new(n).map(|n| (role, n)))
        }))
    }

    fn update<R>(
        &self,
        change: impl FnOnce(&mut Table) -> Result<R, StudioError>,
    ) -> Result<R, StudioError> {
        let io = |e: std::io::Error| StudioError::io(&self.path, &e);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| StudioError::io(dir, &e))?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
            .map_err(io)?;
        file.lock().map_err(io)?;
        let mut table = parse(&read_all(&mut file).map_err(io)?);
        let result = change(&mut table)?;
        let text = serde_json::to_string_pretty(&table)
            .map_err(|e| StudioError::invalid(e.to_string()))?;
        file.set_len(0).map_err(io)?;
        file.seek(SeekFrom::Start(0)).map_err(io)?;
        file.write_all(text.as_bytes()).map_err(io)?;
        Ok(result)
    }

    fn read_table(&self) -> Result<Table, StudioError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Table::new()),
            Err(e) => Err(StudioError::io(&self.path, &e)),
        }
    }
}

fn read_all(file: &mut File) -> std::io::Result<String> {
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

/// A missing or unreadable table is empty (slots are runtime state, safe to rebuild).
fn parse(text: &str) -> Table {
    serde_json::from_str(text).unwrap_or_default()
}
