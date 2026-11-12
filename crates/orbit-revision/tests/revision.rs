//! Revision scenarios from `docs/architecture/07-consolidation.md`.

use chrono::{DateTime, Duration, TimeZone, Utc};
use orbit_core::{AuthorRole, Basis, ClaimId, Explicitness, Integrity, Lifecycle, Scope, SpeechAct, Standing};
use orbit_revision::{
    Cardinality, Claim, LinkKind, PredicateInfo, Relation, RelationJudge, ReplayError, ReviewReason, StructuredOnly,
    Temporality, Value, assess, replay,
};
use uuid::Uuid;

const PREFERENCE: PredicateInfo = PredicateInfo {
    cardinality: Cardinality::Functional,
    temporality: Temporality::Fluent,
};
const BIRTHPLACE: PredicateInfo = PredicateInfo {
    cardinality: Cardinality::Functional,
    temporality: Temporality::Rigid,
};
const USES: PredicateInfo = PredicateInfo {
    cardinality: Cardinality::Multi,
    temporality: Temporality::Fluent,
};

fn user() -> Uuid {
    Uuid::from_u128(1)
}

fn value(n: u128) -> Value {
    Value::Entity(Uuid::from_u128(n))
}

const PYTHON: u128 = 10;
const RUST: u128 = 11;
const GO: u128 = 12;

fn day(n: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap() + Duration::days(n)
}

/// A typed, explicit statement by the user.
fn said(object: Value, scope: Scope, on: i64) -> Claim {
    Claim {
        id: ClaimId::generate(),
        speaker: AuthorRole::User,
        act: SpeechAct::Prefer,
        subject: user(),
        predicate: "preferred_language".into(),
        object,
        negated: false,
        scope,
        asserted_at: day(on),
        valid_hint: None,
        integrity: Integrity::UserDirect,
        explicitness: Explicitness::Explicit,
        extract_confidence: 0.9,
        independence_group: format!("day-{on}"),
        corrects: None,
    }
}

fn systems() -> Scope {
    Scope::general().with("domain", "systems")
}

#[test]
fn a_narrower_preference_is_an_exception_not_a_replacement() {
    let python = said(value(PYTHON), Scope::general(), 0);
    let rust = said(value(RUST), systems(), 150);
    let state = replay(&[python.clone(), rust.clone()], &PREFERENCE, &StructuredOnly).unwrap();

    assert_eq!(state.current().count(), 2);
    let r = state.get(rust.id).unwrap();
    assert_eq!(r.links[0].kind, LinkKind::Specializes);
    assert_eq!(r.links[0].target, python.id);

    let in_systems = state.applicable(&systems());
    assert_eq!(in_systems.len(), 1);
    assert_eq!(in_systems[0].object, value(RUST));
    assert_eq!(
        state.applicable(&Scope::general().with("domain", "data"))[0].object,
        value(PYTHON)
    );
}

#[test]
fn a_later_general_statement_supersedes_and_keeps_history() {
    let python = said(value(PYTHON), Scope::general(), 0);
    let rust_systems = said(value(RUST), systems(), 150);
    let rust_everywhere = said(value(RUST), Scope::general(), 200);
    let state = replay(
        &[python.clone(), rust_systems.clone(), rust_everywhere.clone()],
        &PREFERENCE,
        &StructuredOnly,
    )
    .unwrap();

    let old = state.get(python.id).unwrap();
    assert_eq!(old.lifecycle, Lifecycle::Superseded);
    assert_eq!(old.valid_to, Some(day(200)));
    assert!(old.valid_at(day(100)), "what was true in the past stays answerable");
    assert!(!old.valid_at(day(250)));
    assert!(state.get(rust_systems.id).unwrap().is_current());
    assert_eq!(
        state.get(rust_everywhere.id).unwrap().links[0].kind,
        LinkKind::Supersedes
    );
}

#[test]
fn a_correction_retracts_and_holds_from_the_start() {
    let mut limit = said(Value::Text("100".into()), Scope::general(), 1);
    limit.predicate = "api_limit".into();
    let mut fix = said(Value::Text("200".into()), Scope::general(), 3);
    fix.predicate = "api_limit".into();
    fix.act = SpeechAct::Correct;
    fix.corrects = Some(limit.id);

    let state = replay(&[limit.clone(), fix.clone()], &PREFERENCE, &StructuredOnly).unwrap();

    let wrong = state.get(limit.id).unwrap();
    assert_eq!(wrong.lifecycle, Lifecycle::Retracted);
    assert_eq!(wrong.ended_by, Some(fix.id));
    let right = state.get(fix.id).unwrap();
    assert_eq!(right.valid_from, day(1));
    assert_eq!(right.links[0].kind, LinkKind::Corrects);
}

#[test]
fn a_less_trusted_correction_goes_to_review() {
    let rust = said(value(RUST), Scope::general(), 1);
    let mut relayed = said(value(GO), Scope::general(), 2);
    relayed.act = SpeechAct::Correct;
    relayed.integrity = Integrity::Agent;

    let state = replay(&[rust.clone(), relayed.clone()], &PREFERENCE, &StructuredOnly).unwrap();

    assert!(state.get(rust.id).unwrap().is_current());
    assert_eq!(state.get(rust.id).unwrap().attacks, [relayed.id]);
    assert_eq!(state.reviews[0].reason, ReviewReason::WeakCorrection);
}

#[test]
fn a_relayed_claim_cannot_supersede_what_the_user_said() {
    let rust = said(value(RUST), Scope::general(), 1);
    let mut relayed = said(value(GO), Scope::general(), 5);
    relayed.integrity = Integrity::UserRelayed;

    let state = replay(&[rust.clone(), relayed.clone()], &PREFERENCE, &StructuredOnly).unwrap();

    assert_eq!(state.current().count(), 1);
    assert_eq!(state.get(rust.id).unwrap().attacks, [relayed.id]);
}

#[test]
fn equally_credible_conflicts_on_a_fact_that_cannot_change_become_a_dispute() {
    let mut a = said(value(PYTHON), Scope::general(), 1);
    a.predicate = "birthplace".into();
    let mut b = said(value(RUST), Scope::general(), 2);
    b.predicate = "birthplace".into();

    let state = replay(&[a.clone(), b.clone()], &BIRTHPLACE, &StructuredOnly).unwrap();

    assert_eq!(state.current().count(), 2);
    assert!(state.current().all(|v| v.disputed));
    assert_eq!(state.reviews[0].reason, ReviewReason::Dispute);
    assert_eq!(
        assess(state.get(a.id).unwrap(), &[a.clone(), b.clone()]).standing,
        Standing::Disputed
    );
}

#[test]
fn saying_it_on_two_days_confirms_it_saying_it_twice_in_one_does_not() {
    let first = said(value(RUST), Scope::general(), 1);
    let mut same_day = said(value(RUST), Scope::general(), 1);
    same_day.asserted_at = day(1) + Duration::hours(1);
    let next_day = said(value(RUST), Scope::general(), 2);

    let once = [first.clone(), same_day];
    let state = replay(&once, &PREFERENCE, &StructuredOnly).unwrap();
    let v = state.get(first.id).unwrap();
    assert_eq!(v.supports.len(), 2);
    assert_eq!(assess(v, &once).standing, Standing::Unconfirmed);

    let twice = [first.clone(), next_day];
    let state = replay(&twice, &PREFERENCE, &StructuredOnly).unwrap();
    let a = assess(state.get(first.id).unwrap(), &twice);
    assert_eq!((a.basis, a.standing), (Basis::Stated, Standing::Confirmed));
}

#[test]
fn what_an_assistant_said_is_reported_not_stated() {
    let mut c = said(value(RUST), Scope::general(), 1);
    c.speaker = AuthorRole::Assistant;
    c.integrity = Integrity::Agent;
    let state = replay(std::slice::from_ref(&c), &PREFERENCE, &StructuredOnly).unwrap();
    assert_eq!(assess(state.get(c.id).unwrap(), &[c]).basis, Basis::Reported);
}

#[test]
fn saying_no_longer_closes_the_old_value() {
    let python = said(value(PYTHON), Scope::general(), 1);
    let mut no_longer = said(value(PYTHON), Scope::general(), 30);
    no_longer.negated = true;

    let state = replay(&[python.clone(), no_longer.clone()], &PREFERENCE, &StructuredOnly).unwrap();

    assert_eq!(state.get(python.id).unwrap().lifecycle, Lifecycle::Superseded);
    assert!(state.get(no_longer.id).unwrap().negated);
}

#[test]
fn several_values_can_hold_for_a_multi_valued_predicate() {
    let mut a = said(value(RUST), Scope::general(), 1);
    a.predicate = "uses".into();
    let mut b = said(value(GO), Scope::general(), 2);
    b.predicate = "uses".into();
    let state = replay(&[a, b], &USES, &StructuredOnly).unwrap();
    assert_eq!(state.current().count(), 2);
}

#[test]
fn a_refinement_replaces_the_statement_and_keeps_its_support() {
    struct Refining;
    impl RelationJudge for Refining {
        fn judge(&self, _: &Value, _: &Value) -> Relation {
            Relation::Refines
        }
    }
    let rough = said(Value::Text("prefers small designs".into()), Scope::general(), 1);
    let precise = said(
        Value::Text("prefers small designs with few dependencies".into()),
        Scope::general(),
        9,
    );

    let state = replay(&[rough.clone(), precise.clone()], &PREFERENCE, &Refining).unwrap();

    assert_eq!(state.get(rough.id).unwrap().lifecycle, Lifecycle::Refined);
    let new = state.get(precise.id).unwrap();
    assert_eq!(new.valid_from, day(1));
    assert_eq!(new.supports, [rough.id, precise.id]);
}

#[test]
fn the_result_does_not_depend_on_arrival_order() {
    let claims = [
        said(value(PYTHON), Scope::general(), 0),
        said(value(RUST), systems(), 150),
        said(value(GO), Scope::general().with("project", "atlas"), 160),
        said(value(RUST), Scope::general(), 200),
        said(value(RUST), Scope::general(), 201),
    ];
    let expected = replay(&claims, &PREFERENCE, &StructuredOnly).unwrap();

    let mut shuffled = claims.to_vec();
    for rotation in 1..shuffled.len() {
        shuffled.rotate_left(1);
        shuffled.swap(0, rotation);
        assert_eq!(replay(&shuffled, &PREFERENCE, &StructuredOnly).unwrap(), expected);
    }
}

#[test]
fn claims_about_different_slots_cannot_be_replayed_together() {
    let a = said(value(RUST), Scope::general(), 1);
    let mut b = said(value(GO), Scope::general(), 2);
    b.predicate = "editor".into();
    assert_eq!(
        replay(&[a, b.clone()], &PREFERENCE, &StructuredOnly),
        Err(ReplayError::MixedFamily(b.id))
    );
}
