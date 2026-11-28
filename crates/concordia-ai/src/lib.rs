//! Runtime AI without an LLM (M4 4.12): a typed blackboard, event-driven behavior trees in a flat
//! array, and utility scoring. Deterministic: randomness only from the caller's seeded RNG.

mod blackboard;
mod bt;
mod engine;
mod llm;
mod module;
mod perception;
mod tick;
mod utility;

pub use blackboard::{BbType, BbValue, Blackboard, BlackboardError};
pub use bt::{
    Abort, BehaviorTree, BtNode, CmpOp, NodeKind, ParallelPolicy, Spec, TaskKind, TreeError,
};
pub use engine::{
    AiCast, AiConfig, AiCursor, AiDecision, AiRun, AiStats, Brain, THINK, Trees, add_ai_processors,
    insert_ai_resources,
};
pub use llm::{
    DialogueRequest, LlmPoll, LlmProvider, LlmRequest, LlmService, LlmTicket, MockLlmProvider,
    MockReply, PlannerReply, PlannerRequest, action_schema, validate_reply,
};
pub use module::{
    AiInspect, AiInspectParams, AiInspectResult, AiModule, AiTrace, AiTraceParams, AiTraceResult,
    RunningNode, TracedTick,
};
pub use perception::{
    AiDamaged, AiNoise, Faction, Memory, Perceiver, Remembered, SENSE, Sense, SenseCursor,
    add_perception,
};
pub use tick::{BtState, Decision, NodeMem, Status, TaskHost, Ticker};
pub use utility::{Choice, Consideration, Curve, Input, Selection, UtilityOption, choose, score};
