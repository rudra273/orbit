//! Basis, standing and confidence of a version, computed from its justification.
//!
//! Never reported by a model. Repetition within one independence group counts once.
//! The raw confidence is calibrated later against labelled outcomes.

use crate::{model::Claim, state::BeliefVersion};
use orbit_core::{AuthorRole, Basis, ClaimId, Explicitness, Integrity, SpeechAct, Standing};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Assessment {
    pub basis: Basis,
    pub standing: Standing,
    /// Uncalibrated, `0.0..=1.0`.
    pub confidence: f32,
    /// The strongest single justification's channel.
    pub integrity: Integrity,
}

pub fn assess(version: &BeliefVersion, claims: &[Claim]) -> Assessment {
    let by_id: HashMap<ClaimId, &Claim> = claims.iter().map(|c| (c.id, c)).collect();
    let supports: Vec<&Claim> = version
        .supports
        .iter()
        .filter_map(|id| by_id.get(id).copied())
        .collect();
    let attacks: Vec<&Claim> = version.attacks.iter().filter_map(|id| by_id.get(id).copied()).collect();

    let basis = supports.iter().map(|c| basis_of(c)).max().unwrap_or(Basis::Assumed);
    let integrity = supports
        .iter()
        .map(|c| c.integrity)
        .max()
        .unwrap_or(Integrity::External);

    let user_confirmed = supports
        .iter()
        .any(|c| c.act == SpeechAct::Ratify && c.speaker == AuthorRole::User && c.integrity == Integrity::UserDirect);
    let stated_groups = groups(supports.iter().copied().filter(|c| basis_of(c) == Basis::Stated)).len();
    let standing = if version.disputed {
        Standing::Disputed
    } else if user_confirmed || stated_groups >= 2 {
        Standing::Confirmed
    } else {
        Standing::Unconfirmed
    };

    let support = 1.0 - groups(supports.into_iter()).values().map(|w| 1.0 - w).product::<f32>();
    let doubt: f32 = groups(attacks.into_iter()).values().map(|a| 1.0 - a).product();
    Assessment {
        basis,
        standing,
        confidence: (support * doubt).clamp(0.0, 1.0),
        integrity,
    }
}

fn basis_of(c: &Claim) -> Basis {
    match (c.explicitness, c.speaker, c.integrity) {
        (Explicitness::Implicit, ..) => Basis::Assumed,
        (_, AuthorRole::User, Integrity::UserDirect | Integrity::UserArtifact) => Basis::Stated,
        (_, AuthorRole::Tool, Integrity::UserArtifact) => Basis::Observed,
        _ => Basis::Reported,
    }
}

/// The best weight per independence group.
fn groups<'a>(claims: impl Iterator<Item = &'a Claim>) -> BTreeMap<&'a str, f32> {
    let mut best: BTreeMap<&str, f32> = BTreeMap::new();
    for c in claims {
        let w = weight(c);
        let slot = best.entry(c.independence_group.as_str()).or_insert(0.0);
        *slot = slot.max(w);
    }
    best
}

fn weight(c: &Claim) -> f32 {
    let channel = match c.integrity {
        Integrity::UserDirect => 0.95,
        Integrity::UserArtifact => 0.9,
        Integrity::UserRelayed => 0.7,
        Integrity::Agent => 0.4,
        Integrity::External => 0.3,
    };
    let said = if c.explicitness == Explicitness::Explicit {
        1.0
    } else {
        0.5
    };
    channel * said * c.extract_confidence.clamp(0.0, 1.0)
}
