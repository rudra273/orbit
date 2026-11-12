//! Anchor-verified extraction: the checks a model's claim proposal must pass.
//!
//! The model interprets; these checks make sure it did not invent text, put words
//! in the wrong mouth, or upgrade an implication to a statement.
//! See `docs/architecture/06-memory-formation.md` section 4.

use crate::{
    locate::locate,
    template::{HandleKind, parse_handles, validate},
};
use orbit_core::{Anchor, AuthorRole, EvidenceId, Explicitness, Integrity, Sensitivity};
use std::collections::HashMap;

/// One piece of evidence in the episode being extracted from.
#[derive(Clone, Debug)]
pub struct EvidenceText<'a> {
    pub id: EvidenceId,
    pub author: AuthorRole,
    pub integrity: Integrity,
    pub sensitivity: Sensitivity,
    pub content: &'a str,
    /// Byte ranges detected as pasted or quoted third-party text.
    pub quoted: &'a [(usize, usize)],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteRole {
    /// Words that carry the claim; must be the speaker's own.
    Content,
    /// Words the claim refers to, such as the proposal a user ratifies; anyone's.
    Referenced,
}

#[derive(Clone, Debug)]
pub struct Quote {
    pub text: String,
    pub role: QuoteRole,
}

/// What a model proposed. Slots such as subject and predicate travel alongside,
/// untouched by verification.
#[derive(Clone, Debug)]
pub struct Proposal {
    pub speaker: AuthorRole,
    pub quotes: Vec<Quote>,
    /// Self-contained statement, with handles.
    pub statement_tpl: String,
}

/// Where a literal handle shown to the model points.
#[derive(Clone, Copy, Debug)]
pub struct LiteralRef {
    pub evidence_id: EvidenceId,
}

/// Scores how strongly a premise entails a hypothesis, in `0.0..=1.0`. A model call.
pub trait Entailment {
    fn score(&self, premise: &str, hypothesis: &str) -> f32;
}

#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    pub explicit: f32,
    pub implicit: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Rejection {
    NoContentQuote,
    Unanchored(String),
    Misattributed(String),
    InvalidTemplate(String),
    ForeignLiteral(String),
    NotEntailed(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verified {
    pub anchors: Vec<(Anchor, QuoteRole)>,
    pub explicitness: Explicitness,
    pub integrity: Integrity,
    pub sensitivity: Sensitivity,
    pub entailment: f32,
}

/// Runs every check, in order. `render` turns the template into plain text for entailment.
pub fn verify(
    proposal: &Proposal,
    episode: &[EvidenceText<'_>],
    literals: &HashMap<String, LiteralRef>,
    render: impl Fn(&str) -> Option<String>,
    entailment: &dyn Entailment,
    thresholds: Thresholds,
) -> Result<Verified, Rejection> {
    if !proposal.quotes.iter().any(|q| q.role == QuoteRole::Content) {
        return Err(Rejection::NoContentQuote);
    }

    // 1. Every quote is found verbatim, and content quotes are the speaker's own words.
    let mut anchors = Vec::new();
    let mut used: Vec<&EvidenceText<'_>> = Vec::new();
    let mut integrity: Option<Integrity> = None;
    let mut sensitivity = Sensitivity::Public;
    for quote in &proposal.quotes {
        let (ev, start, end) = episode
            .iter()
            .find_map(|ev| locate(ev.content, &quote.text).map(|(s, e)| (ev, s, e)))
            .ok_or_else(|| Rejection::Unanchored(quote.text.clone()))?;
        let pasted = ev.quoted.iter().any(|&(s, e)| start < e && s < end);
        if quote.role == QuoteRole::Content {
            if pasted || ev.author != proposal.speaker {
                return Err(Rejection::Misattributed(quote.text.clone()));
            }
            integrity = Some(integrity.map_or(ev.integrity, |i| i.min(ev.integrity)));
        }
        sensitivity = sensitivity.max(ev.sensitivity);
        let anchor = Anchor::new(ev.id, ev.content.as_bytes(), start as u32, end as u32)
            .map_err(|e| Rejection::Unanchored(e.to_string()))?;
        anchors.push((anchor, quote.role));
        if !used.iter().any(|u| u.id == ev.id) {
            used.push(ev);
        }
    }

    // 2. The statement only uses literals from the evidence it cites.
    let handles = parse_handles(&proposal.statement_tpl).map_err(|e| Rejection::InvalidTemplate(e.to_string()))?;
    for h in handles.iter().filter(|h| h.kind == HandleKind::Literal) {
        let cited = literals
            .get(&h.key)
            .is_some_and(|r| used.iter().any(|u| u.id == r.evidence_id));
        if !cited {
            return Err(Rejection::ForeignLiteral(h.key.clone()));
        }
    }
    validate(&proposal.statement_tpl, |h| {
        h.kind != HandleKind::Literal || literals.contains_key(&h.key)
    })
    .map_err(|v| Rejection::InvalidTemplate(format!("{v:?}")))?;

    // 3. The cited evidence entails the statement; how strongly decides explicitness.
    let premise = used.iter().map(|u| u.content).collect::<Vec<_>>().join("\n");
    let hypothesis = render(&proposal.statement_tpl)
        .ok_or_else(|| Rejection::InvalidTemplate("statement could not be rendered".into()))?;
    let score = entailment.score(&premise, &hypothesis);
    let explicitness = if score >= thresholds.explicit {
        Explicitness::Explicit
    } else if score >= thresholds.implicit {
        Explicitness::Implicit
    } else {
        return Err(Rejection::NotEntailed(score));
    };

    Ok(Verified {
        anchors,
        explicitness,
        integrity: integrity.expect("at least one content quote was anchored"),
        sensitivity,
        entailment: score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(f32);
    impl Entailment for Fixed {
        fn score(&self, _: &str, _: &str) -> f32 {
            self.0
        }
    }

    const USER_TEXT: &str = "I prefer Rust for the core Orbit engine";
    const ASSISTANT_TEXT: &str = "You could use Go for the engine.";
    const THRESHOLDS: Thresholds = Thresholds {
        explicit: 0.9,
        implicit: 0.6,
    };

    fn episode<'a>(user: EvidenceId, assistant: EvidenceId, quoted: &'a [(usize, usize)]) -> Vec<EvidenceText<'a>> {
        vec![
            EvidenceText {
                id: user,
                author: AuthorRole::User,
                integrity: Integrity::UserDirect,
                sensitivity: Sensitivity::Work,
                content: USER_TEXT,
                quoted,
            },
            EvidenceText {
                id: assistant,
                author: AuthorRole::Assistant,
                integrity: Integrity::Agent,
                sensitivity: Sensitivity::Work,
                content: ASSISTANT_TEXT,
                quoted: &[],
            },
        ]
    }

    fn proposal(speaker: AuthorRole, quote: &str, tpl: &str) -> Proposal {
        Proposal {
            speaker,
            quotes: vec![Quote {
                text: quote.into(),
                role: QuoteRole::Content,
            }],
            statement_tpl: tpl.into(),
        }
    }

    fn run(p: &Proposal, quoted: &[(usize, usize)], score: f32) -> Result<Verified, Rejection> {
        let (u, a) = (EvidenceId::generate(), EvidenceId::generate());
        let literals = HashMap::from([("k1".to_string(), LiteralRef { evidence_id: u })]);
        verify(
            p,
            &episode(u, a, quoted),
            &literals,
            |t| Some(t.replace("⟦lt:k1⟧", "Rust")),
            &Fixed(score),
            THRESHOLDS,
        )
    }

    #[test]
    fn accepts_an_anchored_entailed_claim_by_the_right_speaker() {
        let p = proposal(AuthorRole::User, "I prefer Rust", "Prefers ⟦lt:k1⟧ for the core engine");
        let v = run(&p, &[], 0.95).unwrap();
        assert_eq!(v.explicitness, Explicitness::Explicit);
        assert_eq!(v.integrity, Integrity::UserDirect);
        assert_eq!(
            v.anchors[0].0.resolve(USER_TEXT.as_bytes()),
            Some(&b"I prefer Rust"[..])
        );
    }

    #[test]
    fn rejects_a_quote_that_is_not_in_the_source() {
        let p = proposal(AuthorRole::User, "I love Rust", "Prefers ⟦lt:k1⟧");
        assert_eq!(run(&p, &[], 0.95), Err(Rejection::Unanchored("I love Rust".into())));
    }

    #[test]
    fn rejects_the_assistants_words_attributed_to_the_user() {
        let p = proposal(AuthorRole::User, "You could use Go", "Prefers Go");
        assert!(matches!(run(&p, &[], 0.95), Err(Rejection::Misattributed(_))));
    }

    #[test]
    fn rejects_pasted_text_as_the_users_own_words() {
        let p = proposal(AuthorRole::User, "I prefer Rust", "Prefers ⟦lt:k1⟧");
        assert!(matches!(run(&p, &[(0, 13)], 0.95), Err(Rejection::Misattributed(_))));
    }

    #[test]
    fn rejects_a_literal_typed_by_the_model() {
        let p = proposal(AuthorRole::User, "I prefer Rust", "Prefers Rust `1.99`");
        assert!(matches!(run(&p, &[], 0.95), Err(Rejection::InvalidTemplate(_))));
    }

    #[test]
    fn weak_entailment_makes_a_claim_implicit_and_none_rejects_it() {
        let p = proposal(AuthorRole::User, "I prefer Rust", "Prefers ⟦lt:k1⟧");
        assert_eq!(run(&p, &[], 0.7).unwrap().explicitness, Explicitness::Implicit);
        assert_eq!(run(&p, &[], 0.2), Err(Rejection::NotEntailed(0.2)));
    }

    #[test]
    fn a_ratification_may_reference_the_assistants_proposal() {
        let mut p = proposal(AuthorRole::User, "I prefer Rust", "Prefers ⟦lt:k1⟧");
        p.quotes.push(Quote {
            text: "You could use Go".into(),
            role: QuoteRole::Referenced,
        });
        let v = run(&p, &[], 0.95).unwrap();
        assert_eq!(v.anchors.len(), 2);
        assert_eq!(v.integrity, Integrity::UserDirect);
    }
}
