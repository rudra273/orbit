//! Parsers that turn agent transcripts on disk into ledger events.
//!
//! Pure: a parser takes the text of one session file and returns what to ingest.
//! Reading files and sending events is the CLI's job.

pub mod claude_code;
pub mod codex;

use orbit_ledger::{IncomingEvent, SourceSpec};

/// One session, ready to ingest.
#[derive(Debug)]
pub struct Import {
    pub source: SourceSpec,
    pub events: Vec<IncomingEvent>,
}

/// The result of parsing one file.
#[derive(Debug)]
pub struct Parsed {
    /// `None` when the file holds no conversation.
    pub import: Option<Import>,
    pub skipped: Skipped,
}

/// What a parser left out, and why. Reported, never silently dropped.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Skipped {
    /// Lines that were not valid JSON or lacked a timestamp.
    pub unparsable: usize,
    /// Tool output larger than the configured limit.
    pub oversized: usize,
    /// Records that are not conversation: settings, usage, summaries, reasoning.
    pub not_conversation: usize,
    /// Turns from subagents: assistant-to-assistant work, not the person's conversation.
    pub subagent: usize,
}

impl std::ops::AddAssign for Skipped {
    fn add_assign(&mut self, other: Self) {
        self.unparsable += other.unparsable;
        self.oversized += other.oversized;
        self.not_conversation += other.not_conversation;
        self.subagent += other.subagent;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Tool output above this many bytes is skipped rather than truncated: evidence is never cut.
    pub max_tool_output_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_tool_output_bytes: 16 * 1024,
        }
    }
}

/// Order within a source: the line number, with room for several parts per line.
pub(crate) fn seq(line: usize, part: usize) -> i32 {
    i32::try_from(line * 64 + part.min(63)).unwrap_or(i32::MAX)
}
