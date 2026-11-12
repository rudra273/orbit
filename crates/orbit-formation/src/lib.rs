//! Memory formation: the deterministic checks between raw evidence and claims.
//!
//! Models propose; this crate decides what is admitted. Nothing here calls a model.
//! See `docs/architecture/06-memory-formation.md`.

pub mod literals;
pub mod locate;
pub mod template;
pub mod verify;

pub use literals::{Literal, LiteralKind, detect_literals};
pub use locate::locate;
pub use template::{Handle, HandleKind, TemplateError, Violation, parse_handles, render, validate};
pub use verify::{
    Entailment, EvidenceText, LiteralRef, Proposal, Quote, QuoteRole, Rejection, Thresholds, Verified, verify,
};
