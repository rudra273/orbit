//! One revision step: Scope-Before-Supersede.
//!
//! A conflicting claim is tried as a narrower context first, then as a change over
//! time, then by authority; if none decides, both sides are kept as a dispute.

use crate::{
    model::{Claim, PredicateInfo, Temporality},
    relation::{Relation, RelationJudge, relate},
    state::{BeliefVersion, Link, LinkKind, Review, ReviewReason, SlotState},
};
use orbit_core::{Lifecycle, ScopeRelation, SpeechAct};

pub(crate) fn step(state: &mut SlotState, c: &Claim, predicate: &PredicateInfo, judge: &dyn RelationJudge) {
    if c.act == SpeechAct::Correct && correct(state, c) {
        return;
    }

    for i in closest_first(state, c) {
        let b = &state.versions[i];
        let scope = c.scope.relation_to(&b.scope);
        match relate(predicate, (&c.object, c.negated), (&b.object, b.negated), judge) {
            Relation::Unrelated => {}
            // A restatement in another scope is its own fact, not support for this one.
            Relation::Same | Relation::Refines if scope != ScopeRelation::Equal => {}
            Relation::Same => return reinforce(state, i, c),
            Relation::Refines => return refine(state, i, c),
            Relation::Contradicts => match scope {
                ScopeRelation::Disjoint => {}
                ScopeRelation::Narrower => return open(state, c, Some(link(LinkKind::Specializes, b))),
                ScopeRelation::Broader => return open(state, c, Some(link(LinkKind::Generalizes, b))),
                ScopeRelation::Overlapping => return open(state, c, Some(link(LinkKind::Overlaps, b))),
                ScopeRelation::Equal => return resolve(state, i, c, predicate),
            },
        }
    }
    open(state, c, None);
}

/// Same scope, contradicting values: time, then authority, then dispute.
fn resolve(state: &mut SlotState, i: usize, c: &Claim, predicate: &PredicateInfo) {
    let b = &state.versions[i];
    let (new, old) = (c.authority(), b.authority);

    if predicate.temporality == Temporality::Fluent && new >= old && c.valid_from() > b.valid_from {
        let target = link(LinkKind::Supersedes, b);
        let b = &mut state.versions[i];
        b.lifecycle = Lifecycle::Superseded;
        b.valid_to = Some(c.valid_from());
        b.ended_by = Some(c.id);
        open(state, c, Some(target));
    } else if new > old {
        let target = link(LinkKind::Overrides, b);
        end(state, i, c, Lifecycle::Retracted);
        open(state, c, Some(target));
    } else if new < old {
        state.versions[i].attacks.push(c.id);
    } else {
        let target = link(LinkKind::DisputedWith, b);
        let b = &mut state.versions[i];
        b.disputed = true;
        b.links.push(Link {
            kind: LinkKind::DisputedWith,
            target: c.id,
        });
        state.reviews.push(Review {
            claim: c.id,
            version: b.key,
            reason: ReviewReason::Dispute,
        });
        open(state, c, Some(target));
        state.versions.last_mut().expect("just opened").disputed = true;
    }
}

/// Handles an explicit correction. Returns false when there is nothing to correct,
/// in which case the claim is revised like any other.
fn correct(state: &mut SlotState, c: &Claim) -> bool {
    let by_reference = c.corrects.and_then(|id| {
        state
            .versions
            .iter()
            .position(|v| v.is_current() && v.supports.contains(&id))
    });
    let by_slot = || {
        state
            .versions
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_current() && v.scope == c.scope)
            .max_by_key(|(_, v)| (v.valid_from, v.key))
            .map(|(i, _)| i)
    };
    let Some(i) = by_reference.or_else(by_slot) else {
        return false;
    };

    let target = &state.versions[i];
    if c.authority() < target.authority {
        let key = target.key;
        state.versions[i].attacks.push(c.id);
        state.reviews.push(Review {
            claim: c.id,
            version: key,
            reason: ReviewReason::WeakCorrection,
        });
        return true;
    }

    let (value_retracted_only, valid_from, target_link) = (
        c.negated && c.object == target.object,
        c.valid_hint.unwrap_or(target.valid_from),
        link(LinkKind::Corrects, target),
    );
    end(state, i, c, Lifecycle::Retracted);
    if !value_retracted_only {
        // A correction means the old value was never right: the new one holds from the start.
        open_from(state, c, valid_from, Some(target_link));
    }
    true
}

fn reinforce(state: &mut SlotState, i: usize, c: &Claim) {
    let b = &mut state.versions[i];
    b.supports.push(c.id);
    b.authority = b.authority.max(c.authority());
}

fn refine(state: &mut SlotState, i: usize, c: &Claim) {
    let target = link(LinkKind::Refines, &state.versions[i]);
    let (valid_from, mut supports, authority) = {
        let b = &state.versions[i];
        (b.valid_from, b.supports.clone(), b.authority.max(c.authority()))
    };
    end(state, i, c, Lifecycle::Refined);
    open_from(state, c, valid_from, Some(target));
    let new = state.versions.last_mut().expect("just opened");
    supports.push(c.id);
    new.supports = supports;
    new.authority = authority;
}

fn end(state: &mut SlotState, i: usize, c: &Claim, lifecycle: Lifecycle) {
    let b = &mut state.versions[i];
    b.lifecycle = lifecycle;
    b.ended_by = Some(c.id);
}

fn open(state: &mut SlotState, c: &Claim, link: Option<Link>) {
    open_from(state, c, c.valid_from(), link);
}

fn open_from(state: &mut SlotState, c: &Claim, valid_from: chrono::DateTime<chrono::Utc>, link: Option<Link>) {
    state.versions.push(BeliefVersion {
        key: c.id,
        object: c.object.clone(),
        negated: c.negated,
        scope: c.scope.clone(),
        valid_from,
        valid_to: None,
        lifecycle: Lifecycle::Current,
        ended_by: None,
        disputed: false,
        authority: c.authority(),
        supports: vec![c.id],
        attacks: Vec::new(),
        links: link.into_iter().collect(),
    });
}

fn link(kind: LinkKind, target: &BeliefVersion) -> Link {
    Link {
        kind,
        target: target.key,
    }
}

/// Current versions, same scope first, then nested, overlapping and disjoint ones.
fn closest_first(state: &SlotState, c: &Claim) -> Vec<usize> {
    let rank = |r: ScopeRelation| match r {
        ScopeRelation::Equal => 0,
        ScopeRelation::Narrower | ScopeRelation::Broader => 1,
        ScopeRelation::Overlapping => 2,
        ScopeRelation::Disjoint => 3,
    };
    let mut order: Vec<usize> = (0..state.versions.len())
        .filter(|&i| state.versions[i].is_current())
        .collect();
    order.sort_by_key(|&i| {
        (
            rank(c.scope.relation_to(&state.versions[i].scope)),
            state.versions[i].key,
        )
    });
    order
}
