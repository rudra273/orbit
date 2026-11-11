//! Typed, time-ordered ids. Shown to clients with a prefix: `ev_…`, `bl_…`.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    #[error("expected an id starting with `{0}_`")]
    WrongPrefix(&'static str),
    #[error("malformed id")]
    Malformed,
}

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            /// A new UUIDv7, so ids sort by creation time.
            pub fn generate() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0.simple())
            }
        }

        impl FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let rest = s
                    .strip_prefix($prefix)
                    .and_then(|r| r.strip_prefix('_'))
                    .ok_or(IdError::WrongPrefix($prefix))?;
                Uuid::parse_str(rest).map(Self).map_err(|_| IdError::Malformed)
            }
        }
    };
}

typed_id!(SourceId, "src");
typed_id!(EvidenceId, "ev");
typed_id!(EpisodeId, "ep");
typed_id!(ClaimId, "cl");
typed_id!(BeliefId, "bl");
typed_id!(LiteralId, "lt");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_its_display_form() {
        let id = EvidenceId::generate();
        let shown = id.to_string();
        assert!(shown.starts_with("ev_"));
        assert_eq!(shown.parse::<EvidenceId>().unwrap(), id);
    }

    #[test]
    fn rejects_another_kind_of_id() {
        let id = BeliefId::generate().to_string();
        assert_eq!(id.parse::<EvidenceId>(), Err(IdError::WrongPrefix("ev")));
    }
}
