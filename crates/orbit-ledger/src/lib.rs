//! The evidence ledger: admission, then an append-only write.
//!
//! See `docs/architecture/06-memory-formation.md` (stages 1 and 2).

pub mod admit;
pub mod secrets;
pub mod store;

pub use admit::{AdmissionError, Admitted, IncomingEvent, admit};
pub use store::{Appended, Ledger, LedgerError, SourceSpec};

/// Embedded schema migrations, applied at start-up.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
