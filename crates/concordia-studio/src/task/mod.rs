//! The task board: `studio/tasks/<id>-<slug>.task.md`, its state machine, gates and journal.

mod board;
mod desk;
mod model;
mod question;
mod state;
mod store;

pub use board::{BoardError, NewTask, TaskBoard, TaskEdit, counts};
pub use desk::NewQuestion;
pub use model::{Acceptance, Priority, Task, TaskId, TaskMeta, TaskStatus};
pub(crate) use question::fold;
pub use question::{Question, QuestionAnswer, QuestionEscalation, QuestionStatus};
pub use state::{can_move, next_statuses};
pub use store::{LOCAL_HUMAN, TaskStore};
