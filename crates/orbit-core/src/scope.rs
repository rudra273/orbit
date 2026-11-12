//! Scope: the context a belief holds in. More constraints means more specific.
//!
//! See `docs/architecture/05-memory-model.md` section 5.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A set of `key = value` constraints, such as `{project: orbit, component: engine}`.
/// The empty scope is the general one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Scope(BTreeMap<String, String>);

/// How one scope relates to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeRelation {
    Equal,
    /// Has every constraint of the other, and more.
    Narrower,
    /// The other has every constraint of this one, and more.
    Broader,
    /// Some key has different values, so no context satisfies both.
    Disjoint,
    /// Neither contains the other, yet some context satisfies both.
    Overlapping,
}

impl Scope {
    pub fn general() -> Self {
        Self::default()
    }

    /// Adds one constraint, builder-style.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn is_general(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of constraints; a rough measure of specificity.
    pub fn specificity(&self) -> usize {
        self.0.len()
    }

    /// Canonical text form, stable across runs; used as a database key.
    pub fn key(&self) -> String {
        serde_json::to_string(&self.0).expect("a map of strings always serialises")
    }

    /// Whether a context satisfies every constraint of this scope.
    pub fn applies_to(&self, context: &Scope) -> bool {
        self.0.iter().all(|(k, v)| context.0.get(k) == Some(v))
    }

    pub fn relation_to(&self, other: &Scope) -> ScopeRelation {
        if self == other {
            ScopeRelation::Equal
        } else if self.0.iter().any(|(k, v)| other.0.get(k).is_some_and(|w| w != v)) {
            ScopeRelation::Disjoint
        } else if other.applies_to(self) {
            ScopeRelation::Narrower
        } else if self.applies_to(other) {
            ScopeRelation::Broader
        } else {
            ScopeRelation::Overlapping
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orbit() -> Scope {
        Scope::general().with("project", "orbit")
    }

    #[test]
    fn a_scope_with_more_constraints_is_narrower() {
        let engine = orbit().with("component", "engine");
        assert_eq!(engine.relation_to(&orbit()), ScopeRelation::Narrower);
        assert_eq!(orbit().relation_to(&engine), ScopeRelation::Broader);
        assert_eq!(orbit().relation_to(&Scope::general()), ScopeRelation::Narrower);
    }

    #[test]
    fn different_values_for_one_key_are_disjoint() {
        let atlas = Scope::general().with("project", "atlas");
        assert_eq!(orbit().relation_to(&atlas), ScopeRelation::Disjoint);
    }

    #[test]
    fn unrelated_keys_overlap() {
        let systems = Scope::general().with("domain", "systems");
        assert_eq!(orbit().relation_to(&systems), ScopeRelation::Overlapping);
    }

    #[test]
    fn the_key_is_independent_of_insertion_order() {
        let a = Scope::general().with("b", "2").with("a", "1");
        let b = Scope::general().with("a", "1").with("b", "2");
        assert_eq!(a.key(), b.key());
    }
}
