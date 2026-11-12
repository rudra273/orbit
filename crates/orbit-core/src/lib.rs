//! Pure types shared by every part of Orbit. No I/O lives here.
//!
//! See `docs/architecture/05-memory-model.md` for the concepts.

pub mod anchor;
pub mod chain;
pub mod ids;
pub mod labels;
pub mod scope;

pub use anchor::{Anchor, AnchorError, sha256};
pub use ids::{BeliefId, ClaimId, EpisodeId, EvidenceId, IdError, LiteralId, SourceId};
pub use labels::{AuthorRole, Basis, Explicitness, Integrity, Lifecycle, Sensitivity, SpeechAct, Standing};
pub use scope::{Scope, ScopeRelation};
