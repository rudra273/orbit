//! Belief revision: how claims become versioned beliefs.
//!
//! Pure and deterministic. The state of a slot family is a function of the set of
//! claims about it, replayed in event-time order (INV-9).
//! See `docs/architecture/07-consolidation.md` sections 1 to 3.

pub mod assess;
pub mod model;
pub mod relation;
pub mod replay;
pub mod state;
mod step;

pub use assess::{Assessment, assess};
pub use model::{Authority, Cardinality, Claim, PredicateInfo, Temporality, Value};
pub use relation::{Relation, RelationJudge, StructuredOnly};
pub use replay::{ReplayError, replay};
pub use state::{BeliefVersion, Link, LinkKind, Review, ReviewReason, SlotState};
