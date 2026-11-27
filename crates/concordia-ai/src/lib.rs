//! Runtime AI without an LLM (M4 4.12): a typed blackboard, event-driven behavior trees in a flat
//! array, and utility scoring. Deterministic: randomness only from the caller's seeded RNG.

mod blackboard;
mod bt;
mod tick;
mod utility;

pub use blackboard::{BbType, BbValue, Blackboard, BlackboardError};
pub use bt::{
    Abort, BehaviorTree, BtNode, CmpOp, NodeKind, ParallelPolicy, Spec, TaskKind, TreeError,
};
pub use tick::{BtState, Decision, NodeMem, Status, TaskHost, Ticker};
pub use utility::{Choice, Consideration, Curve, Input, Selection, UtilityOption, choose, score};
