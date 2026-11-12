//! The labels every memory carries. They are computed, never written by a model.
//!
//! Variant order is meaningful: derived items take the *minimum* integrity and
//! the *maximum* sensitivity of what they depend on.

use serde::{Deserialize, Serialize};

macro_rules! label {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($(#[$vdoc])* $variant),+ }

        impl $name {
            /// The spelling used in JSON and in the database enum.
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
    };
}

label!(
    /// How far the channel a piece of evidence arrived through can be trusted. Low to high.
    Integrity {
        External => "external",
        Agent => "agent",
        UserRelayed => "user_relayed",
        UserArtifact => "user_artifact",
        UserDirect => "user_direct",
    }
);

label!(
    /// Who may see it. Low to high.
    Sensitivity {
        Public => "public",
        Work => "work",
        Personal => "personal",
        Sensitive => "sensitive",
    }
);

label!(
    /// How Orbit knows something. Weak to strong.
    Basis {
        Assumed => "assumed",
        Inferred => "inferred",
        Reported => "reported",
        Observed => "observed",
        Stated => "stated",
    }
);

label!(
    Standing {
        Disputed => "disputed",
        Unconfirmed => "unconfirmed",
        Confirmed => "confirmed",
    }
);

label!(
    Lifecycle {
        Current => "current",
        /// Stopped being true in the world; its valid time is closed.
        Superseded => "superseded",
        /// Replaced by a more precise statement of the same thing.
        Refined => "refined",
        Retracted => "retracted",
        Expired => "expired",
    }
);

label!(
    AuthorRole {
        User => "user",
        Assistant => "assistant",
        Tool => "tool",
        System => "system",
        ThirdParty => "third_party",
    }
);

label!(
    /// Whether a claim was said outright or only implied. Weak to strong.
    Explicitness {
        Implicit => "implicit",
        Explicit => "explicit",
    }
);

label!(
    /// What a sentence does: the speech act a claim was made with.
    SpeechAct {
        Assert => "assert",
        Prefer => "prefer",
        Decide => "decide",
        Plan => "plan",
        Commit => "commit",
        Instruct => "instruct",
        Correct => "correct",
        Reject => "reject",
        Ask => "ask",
        Hypothesize => "hypothesize",
        Define => "define",
        Report => "report",
        Evaluate => "evaluate",
        Rationale => "rationale",
        Ratify => "ratify",
    }
);

impl SpeechAct {
    /// How much commitment the act carries, used when two claims conflict.
    pub fn force(self) -> u8 {
        match self {
            Self::Decide | Self::Commit | Self::Instruct | Self::Correct => 3,
            Self::Prefer | Self::Assert | Self::Define | Self::Reject | Self::Plan | Self::Ratify => 2,
            Self::Report | Self::Evaluate | Self::Rationale => 1,
            Self::Ask | Self::Hypothesize => 0,
        }
    }
}

impl Integrity {
    /// Integrity of something derived from several sources: the weakest of them.
    pub fn derived(parts: impl IntoIterator<Item = Integrity>) -> Option<Integrity> {
        parts.into_iter().min()
    }
}

impl Sensitivity {
    /// Sensitivity of something derived from several sources: the strictest of them.
    pub fn derived(parts: impl IntoIterator<Item = Sensitivity>) -> Option<Sensitivity> {
        parts.into_iter().max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_integrity_is_the_weakest_source() {
        let got = Integrity::derived([Integrity::UserDirect, Integrity::Agent, Integrity::UserArtifact]);
        assert_eq!(got, Some(Integrity::Agent));
    }

    #[test]
    fn derived_sensitivity_is_the_strictest_source() {
        let got = Sensitivity::derived([Sensitivity::Work, Sensitivity::Sensitive]);
        assert_eq!(got, Some(Sensitivity::Sensitive));
    }

    #[test]
    fn stated_outranks_inferred() {
        assert!(Basis::Stated > Basis::Inferred);
    }
}
