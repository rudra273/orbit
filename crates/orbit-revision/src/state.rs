//! The result of revision: belief versions in one slot family.

use crate::model::{Authority, Value};
use chrono::{DateTime, Utc};
use orbit_core::{ClaimId, Lifecycle, Scope};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Supersedes,
    Refines,
    /// An exception to a broader default.
    Specializes,
    /// A new default; the narrower version remains its exception.
    Generalizes,
    /// Neither scope contains the other, but both can apply at once.
    Overlaps,
    /// Replaced a version the user said was wrong.
    Corrects,
    /// Replaced a version from a less credible source.
    Overrides,
    DisputedWith,
}

/// A link from one version to another, by the other's key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub kind: LinkKind,
    pub target: ClaimId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeliefVersion {
    /// The claim that opened this version; stable across replays.
    pub key: ClaimId,
    pub object: Value,
    pub negated: bool,
    pub scope: Scope,
    pub valid_from: DateTime<Utc>,
    pub valid_to: Option<DateTime<Utc>>,
    pub lifecycle: Lifecycle,
    /// The claim that ended this version, if it has ended.
    pub ended_by: Option<ClaimId>,
    pub disputed: bool,
    /// The strongest authority among the supports.
    pub authority: Authority,
    pub supports: Vec<ClaimId>,
    pub attacks: Vec<ClaimId>,
    pub links: Vec<Link>,
}

impl BeliefVersion {
    pub fn is_current(&self) -> bool {
        self.lifecycle == Lifecycle::Current
    }

    /// Whether this version held at `at` in the world, as now believed.
    pub fn valid_at(&self, at: DateTime<Utc>) -> bool {
        matches!(self.lifecycle, Lifecycle::Current | Lifecycle::Superseded)
            && self.valid_from <= at
            && self.valid_to.is_none_or(|to| at < to)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewReason {
    /// Two equally credible claims disagree.
    Dispute,
    /// A correction from a less credible source than what it corrects.
    WeakCorrection,
}

/// Something only the owner can settle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Review {
    pub claim: ClaimId,
    pub version: ClaimId,
    pub reason: ReviewReason,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotState {
    pub versions: Vec<BeliefVersion>,
    pub reviews: Vec<Review>,
}

impl SlotState {
    pub fn current(&self) -> impl Iterator<Item = &BeliefVersion> {
        self.versions.iter().filter(|v| v.is_current())
    }

    pub fn get(&self, key: ClaimId) -> Option<&BeliefVersion> {
        self.versions.iter().find(|v| v.key == key)
    }

    /// The current versions that apply in `context`: those with the most specific
    /// scope the context satisfies. More than one means a conflict to report.
    pub fn applicable(&self, context: &Scope) -> Vec<&BeliefVersion> {
        let fitting: Vec<_> = self.current().filter(|v| v.scope.applies_to(context)).collect();
        let best = fitting.iter().map(|v| v.scope.specificity()).max();
        fitting
            .into_iter()
            .filter(|v| Some(v.scope.specificity()) == best)
            .collect()
    }
}
