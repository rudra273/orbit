//! The inputs to revision: verified claims and what is known about their predicate.

use chrono::{DateTime, Utc};
use orbit_core::{AuthorRole, ClaimId, Explicitness, Integrity, LiteralId, Scope, SpeechAct};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a claim says the predicate's value is.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Value {
    Entity(Uuid),
    Literal(LiteralId),
    /// Free text; comparing two of these needs a judgement.
    Text(String),
}

impl Value {
    pub fn is_structured(&self) -> bool {
        !matches!(self, Value::Text(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinality {
    /// One value per scope at a time (preferred language).
    Functional,
    /// Several values may hold together (uses technology).
    Multi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Temporality {
    /// Can change over time (preference, current job).
    Fluent,
    /// Does not change; a conflict means one side is wrong (birthplace).
    Rigid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredicateInfo {
    pub cardinality: Cardinality,
    pub temporality: Temporality,
}

/// A verified claim, as revision sees it. All claims replayed together share
/// `subject` and `predicate`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    pub speaker: AuthorRole,
    pub act: SpeechAct,
    pub subject: Uuid,
    pub predicate: String,
    pub object: Value,
    /// "Not X": the value is denied rather than asserted.
    pub negated: bool,
    pub scope: Scope,
    /// When it was said (event time).
    pub asserted_at: DateTime<Utc>,
    /// When the claim says it became true, if it says so.
    pub valid_hint: Option<DateTime<Utc>>,
    pub integrity: Integrity,
    pub explicitness: Explicitness,
    /// Extractor confidence after verification, `0.0..=1.0`.
    pub extract_confidence: f32,
    /// Repetitions within one group count once (one episode-day).
    pub independence_group: String,
    /// For corrections: the claim being corrected, when known.
    pub corrects: Option<ClaimId>,
}

/// Who wins a conflict, compared in order: channel, explicitness, force of the act.
/// Recency is deliberately absent; time is handled separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Authority {
    pub integrity: Integrity,
    pub explicitness: Explicitness,
    pub force: u8,
}

impl Claim {
    /// When this claim's value starts to hold.
    pub fn valid_from(&self) -> DateTime<Utc> {
        self.valid_hint.unwrap_or(self.asserted_at)
    }

    pub fn authority(&self) -> Authority {
        Authority {
            integrity: self.integrity,
            explicitness: self.explicitness,
            force: self.act.force(),
        }
    }
}
