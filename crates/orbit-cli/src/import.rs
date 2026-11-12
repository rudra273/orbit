//! `orbit import`: find session files, parse them, send them.

use crate::{Source, client::Client};
use orbit_import::{Options, Parsed, Skipped, claude_code, codex};
use std::path::{Path, PathBuf};

pub async fn run(from: &Source, options: &Options, client: Option<&Client>) -> anyhow::Result<()> {
    let (root, parse): (PathBuf, fn(&str, &Options) -> Parsed) = match from {
        Source::ClaudeCode { dir } => (
            dir.clone().unwrap_or_else(|| home(".claude/projects")),
            claude_code::parse,
        ),
        Source::Codex { dir } => (dir.clone().unwrap_or_else(|| home(".codex/sessions")), codex::parse),
    };
    let files = session_files(&root)?;
    println!("{} session files under {}", files.len(), root.display());

    let (mut sessions, mut events, mut new, mut duplicate, mut empty) = (0, 0, 0, 0, 0);
    let mut skipped = Skipped::default();
    for file in &files {
        let text = std::fs::read_to_string(file)?;
        let parsed = parse(&text, options);
        skipped += parsed.skipped;
        let Some(import) = parsed.import else {
            empty += 1;
            continue;
        };
        sessions += 1;
        events += import.events.len();
        if let Some(client) = client {
            let sent = client.send(&import.source, &import.events).await?;
            new += sent.new;
            duplicate += sent.duplicate;
        }
    }

    println!("{sessions} sessions, {events} events ({empty} files had no conversation)");
    println!(
        "skipped: {} oversized tool outputs, {} subagent turns, {} non-conversation records, {} unparsable lines",
        skipped.oversized, skipped.subagent, skipped.not_conversation, skipped.unparsable
    );
    if client.is_some() {
        println!("stored: {new} new, {duplicate} already present");
    }
    Ok(())
}

fn home(rest: &str) -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest)
}

/// Every `.jsonl` file under `root`, sorted.
fn session_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "jsonl") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}
