//! How a new claim's value relates to an existing belief's value.

use crate::model::{Cardinality, PredicateInfo, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Same,
    /// A more precise statement of the same thing.
    Refines,
    Contradicts,
    Unrelated,
}

/// Judges free-text values. Must be deterministic: implementations read cached
/// judgements, so a replay gives the same answer every time.
pub trait RelationJudge {
    fn judge(&self, new: &Value, existing: &Value) -> Relation;
}

/// A judge for slots whose values are all structured; any text pair is unrelated.
pub struct StructuredOnly;

impl RelationJudge for StructuredOnly {
    fn judge(&self, _: &Value, _: &Value) -> Relation {
        Relation::Unrelated
    }
}

pub(crate) fn relate(
    predicate: &PredicateInfo,
    (new, new_negated): (&Value, bool),
    (existing, existing_negated): (&Value, bool),
    judge: &dyn RelationJudge,
) -> Relation {
    if new == existing {
        return if new_negated == existing_negated {
            Relation::Same
        } else {
            Relation::Contradicts
        };
    }
    if new_negated || existing_negated {
        // "Not X" says nothing about Y.
        return Relation::Unrelated;
    }
    if new.is_structured() && existing.is_structured() {
        return match predicate.cardinality {
            Cardinality::Functional => Relation::Contradicts,
            Cardinality::Multi => Relation::Unrelated,
        };
    }
    judge.judge(new, existing)
}
