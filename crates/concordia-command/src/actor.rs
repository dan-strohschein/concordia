//! Who made a change: `<kind>:<name>[#n]`, e.g. `agent:gameplay-programmer#2`.

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use concordia_core::id::SessionId;

const GRAMMAR_HINT: &str = "expected <kind>:<name>[#n], e.g. agent:claude-code";
const MAX_NAME: usize = 64;

/// The kind of actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActorKind {
    /// A person at the editor or CLI.
    Human,
    /// An AI agent.
    Agent,
    /// The engine itself (loaders, migrations).
    System,
    /// A tool or script outside the engine (CLI, importers).
    Tool,
    /// Gameplay script code.
    Script,
}

impl ActorKind {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Human, "human"),
        (Self::Agent, "agent"),
        (Self::System, "system"),
        (Self::Tool, "tool"),
        (Self::Script, "script"),
    ];

    /// The grammar keyword.
    pub fn as_str(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(k, _)| *k == self)
            .map_or("", |(_, s)| s)
    }

    fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, s)| *s == text).map(|(k, _)| *k)
    }
}

/// An actor string did not match the grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActorError {
    /// The input is not `<kind>:<name>[#n]`.
    #[error("invalid actor {input:?}: {hint}")]
    Malformed {
        /// The rejected text.
        input: String,
        /// How to fix it.
        hint: String,
    },
}

/// An attributed actor. Names are lowercase `[a-z0-9][a-z0-9._-]*`, at most 64 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Actor {
    kind: ActorKind,
    name: String,
    instance: Option<NonZeroU32>,
}

impl Actor {
    /// Builds an actor, validating the name.
    pub fn new(
        kind: ActorKind,
        name: &str,
        instance: Option<NonZeroU32>,
    ) -> Result<Self, ActorError> {
        let text = format!("{}:{name}", kind.as_str());
        validate_name(name).map_err(|hint| malformed(&text, hint))?;
        Ok(Self {
            kind,
            name: name.to_owned(),
            instance,
        })
    }

    /// The kind.
    pub fn kind(&self) -> ActorKind {
        self.kind
    }

    /// The name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The instance number, if any.
    pub fn instance(&self) -> Option<NonZeroU32> {
        self.instance
    }

    /// True for `human:*` actors.
    pub fn is_human(&self) -> bool {
        self.kind == ActorKind::Human
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.as_str(), self.name)?;
        match self.instance {
            Some(n) => write!(f, "#{n}"),
            None => Ok(()),
        }
    }
}

impl FromStr for Actor {
    type Err = ActorError;

    fn from_str(s: &str) -> Result<Self, ActorError> {
        let (kind, rest) = s
            .split_once(':')
            .ok_or_else(|| malformed(s, GRAMMAR_HINT))?;
        let kind = ActorKind::parse(kind).ok_or_else(|| {
            malformed(s, "kind must be one of human, agent, system, tool, script")
        })?;
        let (name, instance) =
            match rest.split_once('#') {
                Some((name, n)) => (
                    name,
                    Some(parse_instance(n).ok_or_else(|| {
                        malformed(s, "instance must be a positive integer, e.g. #2")
                    })?),
                ),
                None => (rest, None),
            };
        validate_name(name).map_err(|hint| malformed(s, hint))?;
        Ok(Self {
            kind,
            name: name.to_owned(),
            instance,
        })
    }
}

fn parse_instance(text: &str) -> Option<NonZeroU32> {
    text.bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| text.parse().ok())
        .flatten()
}

fn validate_name(name: &str) -> Result<(), &'static str> {
    let mut bytes = name.bytes();
    match bytes.next() {
        None => Err("name must not be empty"),
        Some(_) if name.len() > MAX_NAME => Err("name must be at most 64 characters"),
        Some(first) if !(first.is_ascii_lowercase() || first.is_ascii_digit()) => {
            Err("name must start with a lowercase letter or digit")
        }
        Some(_)
            if !bytes
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b)) =>
        {
            Err("name may contain only lowercase letters, digits, '.', '_' and '-'")
        }
        Some(_) => Ok(()),
    }
}

fn malformed(input: &str, hint: &str) -> ActorError {
    ActorError::Malformed {
        input: input.to_owned(),
        hint: hint.to_owned(),
    }
}

impl serde::Serialize for Actor {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Actor {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// An actor plus the API session it acted through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    /// Who.
    pub actor: Actor,
    /// Through which session, if any.
    pub session: Option<SessionId>,
}

impl From<Actor> for Author {
    fn from(actor: Actor) -> Self {
        Self {
            actor,
            session: None,
        }
    }
}
