//! Buses: `.buses.ron` layouts (Master, SFX → Master, Music → Master by default) and the
//! validated graph the mixer sums children-first.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::AudioError;

/// The root bus; it always exists.
pub const MASTER: &str = "Master";

/// One bus in a layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BusDef {
    /// Unique name.
    pub name: String,
    /// Gain in decibels, in [−80, 24].
    #[serde(default)]
    pub gain_db: f32,
    /// Silenced.
    #[serde(default)]
    pub mute: bool,
    /// The parent bus; none is Master (Master itself has none).
    #[serde(default)]
    pub send: Option<String>,
}

/// A bus layout (`.buses.ron`); Master is implicit unless listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BusLayout {
    /// The buses.
    pub buses: Vec<BusDef>,
}

impl Default for BusLayout {
    fn default() -> Self {
        let bus = |name: &str| BusDef {
            name: name.into(),
            gain_db: 0.0,
            mute: false,
            send: Some(MASTER.into()),
        };
        Self {
            buses: vec![bus("SFX"), bus("Music")],
        }
    }
}

/// A bus's index in its graph (Master is 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BusIndex(pub(crate) usize);

impl BusIndex {
    /// Master.
    pub const MASTER: Self = Self(0);

    /// The raw index.
    pub fn get(self) -> usize {
        self.0
    }
}

/// A validated layout: names, parents, gains and the children-first summing order.
#[derive(Debug, Clone, PartialEq)]
pub struct BusGraph {
    pub(crate) names: Vec<String>,
    pub(crate) parents: Vec<Option<usize>>,
    pub(crate) gains_db: Vec<f32>,
    pub(crate) mutes: Vec<bool>,
    pub(crate) order: Vec<usize>,
}

impl BusGraph {
    /// Validates `layout`: unique names, known sends, no cycles, gains in range.
    pub fn new(layout: &BusLayout) -> Result<Self, AudioError> {
        let mut graph = Self {
            names: vec![MASTER.into()],
            parents: vec![None],
            gains_db: vec![0.0],
            mutes: vec![false],
            order: Vec::new(),
        };
        let mut master_seen = false;
        for def in &layout.buses {
            check_gain(def)?;
            if def.name == MASTER {
                if master_seen {
                    return Err(AudioError::DuplicateBus(def.name.clone()));
                }
                if def.send.is_some() {
                    return Err(AudioError::Bus {
                        bus: def.name.clone(),
                        reason: "Master is the root and has no send".into(),
                    });
                }
                master_seen = true;
                graph.gains_db[0] = def.gain_db;
                graph.mutes[0] = def.mute;
                continue;
            }
            if def.name.is_empty() || graph.names.contains(&def.name) {
                return Err(AudioError::DuplicateBus(def.name.clone()));
            }
            graph.names.push(def.name.clone());
            graph.parents.push(None);
            graph.gains_db.push(def.gain_db);
            graph.mutes.push(def.mute);
        }
        for def in layout.buses.iter().filter(|d| d.name != MASTER) {
            let send = def.send.as_deref().unwrap_or(MASTER);
            let parent = graph.names.iter().position(|n| n == send).ok_or_else(|| {
                AudioError::UnknownSend {
                    bus: def.name.clone(),
                    send: send.into(),
                }
            })?;
            let me = graph
                .names
                .iter()
                .position(|n| *n == def.name)
                .expect("added above");
            graph.parents[me] = Some(parent);
        }
        graph.order = graph.children_first()?;
        Ok(graph)
    }

    /// Every bus after all of its children, Master last; a cycle is an error naming it.
    fn children_first(&self) -> Result<Vec<usize>, AudioError> {
        for start in 0..self.names.len() {
            let mut path = vec![start];
            let mut at = start;
            while let Some(parent) = self.parents[at] {
                if let Some(i) = path.iter().position(|&p| p == parent) {
                    let mut cycle: Vec<String> =
                        path[i..].iter().map(|&b| self.names[b].clone()).collect();
                    cycle.push(self.names[parent].clone());
                    return Err(AudioError::BusCycle { path: cycle });
                }
                path.push(parent);
                at = parent;
            }
        }
        let depth = |mut b: usize| {
            let mut d = 0;
            while let Some(p) = self.parents[b] {
                d += 1;
                b = p;
            }
            d
        };
        let mut order: Vec<usize> = (0..self.names.len()).collect();
        order.sort_by_key(|&b| (std::cmp::Reverse(depth(b)), b));
        Ok(order)
    }

    /// The bus named `name`.
    pub fn index(&self, name: &str) -> Option<BusIndex> {
        self.names.iter().position(|n| n == name).map(BusIndex)
    }

    /// Every bus name, Master first.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// A bus's parent (none for Master).
    pub fn parent(&self, bus: BusIndex) -> Option<BusIndex> {
        self.parents.get(bus.0).copied().flatten().map(BusIndex)
    }

    /// A bus's layout gain in decibels.
    pub fn gain_db(&self, bus: BusIndex) -> f32 {
        self.gains_db.get(bus.0).copied().unwrap_or(0.0)
    }

    /// Whether a bus is muted in the layout.
    pub fn muted(&self, bus: BusIndex) -> bool {
        self.mutes.get(bus.0).copied().unwrap_or(false)
    }

    /// The number of buses.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Never true: Master always exists.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

fn check_gain(def: &BusDef) -> Result<(), AudioError> {
    if def.gain_db.is_finite() && (-80.0..=24.0).contains(&def.gain_db) {
        Ok(())
    } else {
        Err(AudioError::Bus {
            bus: def.name.clone(),
            reason: format!("gain_db {} must be in [-80, 24]", def.gain_db),
        })
    }
}

/// Reads a `.buses.ron` layout and validates it.
pub fn read_buses(text: &str) -> Result<BusLayout, AudioError> {
    let layout = concordia_scene::read_document::<BusLayout>(text)
        .map_err(|e| AudioError::Parse(e.to_string()))?;
    BusGraph::new(&layout)?;
    Ok(layout)
}

/// Writes a layout as canonical `.buses.ron` text.
pub fn write_buses(layout: &BusLayout) -> Result<String, AudioError> {
    BusGraph::new(layout)?;
    concordia_scene::write_document(layout).map_err(|e| AudioError::Parse(e.to_string()))
}
