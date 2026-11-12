//! Slot replay: recompute a slot family from all of its claims.

use crate::{model::Claim, model::PredicateInfo, relation::RelationJudge, state::SlotState, step::step};
use orbit_core::ClaimId;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("claim {0} has a different subject or predicate from the rest")]
    MixedFamily(ClaimId),
}

/// Folds the claims in event-time order. The result depends only on the set of
/// claims, never on the order they arrived in (INV-9).
///
/// Callers exclude retracted claims and echoes of what Orbit served before calling.
pub fn replay(
    claims: &[Claim],
    predicate: &PredicateInfo,
    judge: &dyn RelationJudge,
) -> Result<SlotState, ReplayError> {
    let mut ordered: Vec<&Claim> = claims.iter().collect();
    ordered.sort_by_key(|c| (c.asserted_at, c.id));

    if let Some(first) = ordered.first()
        && let Some(odd) = ordered
            .iter()
            .find(|c| c.subject != first.subject || c.predicate != first.predicate)
    {
        return Err(ReplayError::MixedFamily(odd.id));
    }

    let mut state = SlotState::default();
    for c in ordered {
        step(&mut state, c, predicate, judge);
    }
    Ok(state)
}
