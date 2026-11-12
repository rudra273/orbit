//! Claude Code session transcripts (`~/.claude/projects/<project>/<session>.jsonl`).
//!
//! One JSON object per line. Only `user` and `assistant` lines are conversation.
//! A `user` line is the person's own words only when the client did not inject it.

use crate::{Import, Options, Parsed, Skipped, seq};
use chrono::{DateTime, Utc};
use orbit_core::{AuthorRole, Integrity, Sensitivity};
use orbit_ledger::{IncomingEvent, SourceSpec};
use serde_json::{Map, Value};

const CLIENT: &str = "claude-code";

/// Parses one session file. `import` is `None` when it holds no conversation.
pub fn parse(text: &str, options: &Options) -> Parsed {
    let mut events = Vec::new();
    let mut skipped = Skipped::default();
    let (mut session, mut app, mut model, mut title, mut ai_title) = (None, None, None, None, None);
    let mut meta = Map::new();

    for (n, line) in text.lines().enumerate() {
        let Ok(o) = serde_json::from_str::<Value>(line) else {
            skipped.unparsable += 1;
            continue;
        };
        match o["type"].as_str() {
            Some("custom-title") => title = o["customTitle"].as_str().map(str::to_string),
            Some("ai-title") => ai_title = o["aiTitle"].as_str().map(str::to_string),
            Some(kind @ ("user" | "assistant")) => {
                if o["isSidechain"].as_bool() == Some(true) {
                    skipped.subagent += 1;
                    continue;
                }
                if o["isCompactSummary"].as_bool() == Some(true) {
                    skipped.not_conversation += 1;
                    continue;
                }
                let (Some(uuid), Some(at)) = (o["uuid"].as_str(), timestamp(&o["timestamp"])) else {
                    skipped.unparsable += 1;
                    continue;
                };
                session = session.or_else(|| o["sessionId"].as_str().map(str::to_string));
                app = app.or_else(|| o["entrypoint"].as_str().map(str::to_string));
                for (key, field) in [("cwd", "cwd"), ("git_branch", "gitBranch")] {
                    if let Some(v) = o[field].as_str() {
                        meta.entry(key).or_insert_with(|| Value::String(v.to_string()));
                    }
                }
                let message = &o["message"];
                if kind == "assistant" {
                    if let Some(m) = message["model"].as_str() {
                        model = Some(m.to_string());
                    }
                    assistant(message, uuid, n, at, model.as_deref(), &mut events, &mut skipped);
                } else {
                    user(&o, message, uuid, n, at, options, &mut events, &mut skipped);
                }
            }
            _ => skipped.not_conversation += 1,
        }
    }

    let Some(session) = session.filter(|_| !events.is_empty()) else {
        return Parsed { import: None, skipped };
    };
    meta.insert("client".into(), Value::String(CLIENT.into()));
    let import = Import {
        source: SourceSpec {
            kind: "conversation".into(),
            external_ref: format!("{CLIENT}:{session}"),
            app,
            model,
            title: title.or(ai_title),
            meta,
        },
        events,
    };
    Parsed {
        import: Some(import),
        skipped,
    }
}

#[allow(clippy::too_many_arguments)]
fn user(
    line: &Value,
    message: &Value,
    uuid: &str,
    n: usize,
    at: DateTime<Utc>,
    options: &Options,
    events: &mut Vec<IncomingEvent>,
    skipped: &mut Skipped,
) {
    let author = if injected(line) {
        AuthorRole::System
    } else {
        AuthorRole::User
    };
    match &message["content"] {
        Value::String(s) => {
            let role = if s.trim_start().starts_with("<local-command-stdout>") {
                AuthorRole::Tool
            } else {
                author
            };
            push(events, uuid, n, 0, at, role, None, s.clone());
        }
        Value::Array(blocks) => {
            for (i, b) in blocks.iter().enumerate() {
                match b["type"].as_str() {
                    Some("text") => push(
                        events,
                        uuid,
                        n,
                        i,
                        at,
                        author,
                        None,
                        b["text"].as_str().unwrap_or("").into(),
                    ),
                    Some("tool_result") => {
                        let text = tool_result_text(&b["content"]);
                        if text.len() > options.max_tool_output_bytes {
                            skipped.oversized += 1;
                        } else {
                            push(
                                events,
                                uuid,
                                n,
                                i,
                                at,
                                AuthorRole::Tool,
                                b["tool_use_id"].as_str().map(str::to_string),
                                text,
                            );
                        }
                    }
                    _ => skipped.not_conversation += 1,
                }
            }
        }
        _ => skipped.not_conversation += 1,
    }
}

fn assistant(
    message: &Value,
    uuid: &str,
    n: usize,
    at: DateTime<Utc>,
    model: Option<&str>,
    events: &mut Vec<IncomingEvent>,
    skipped: &mut Skipped,
) {
    let Some(blocks) = message["content"].as_array() else {
        return;
    };
    for (i, b) in blocks.iter().enumerate() {
        match b["type"].as_str() {
            Some("text") => {
                let text = b["text"].as_str().unwrap_or("").to_string();
                push(
                    events,
                    uuid,
                    n,
                    i,
                    at,
                    AuthorRole::Assistant,
                    model.map(str::to_string),
                    text,
                );
            }
            Some("tool_use") => {
                // The assistant's action. Its input is stored as compact JSON.
                let name = b["name"].as_str().unwrap_or("tool");
                let input = serde_json::to_string(&b["input"]).unwrap_or_default();
                push(
                    events,
                    uuid,
                    n,
                    i,
                    at,
                    AuthorRole::Assistant,
                    Some(format!("tool_use:{name}")),
                    input,
                );
            }
            _ => skipped.not_conversation += 1,
        }
    }
}

/// Text the client added on the person's behalf: notifications, hook output, meta lines.
fn injected(line: &Value) -> bool {
    let origin = line["origin"]["kind"].as_str().or(line["turnOrigin"].as_str());
    line["isMeta"].as_bool() == Some(true)
        || line["promptSource"].as_str() == Some("system")
        || matches!(origin, Some("task-notification" | "task_notification" | "peer"))
}

fn tool_result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    events: &mut Vec<IncomingEvent>,
    uuid: &str,
    line: usize,
    part: usize,
    at: DateTime<Utc>,
    author: AuthorRole,
    author_ref: Option<String>,
    content: String,
) {
    if content.trim().is_empty() {
        return;
    }
    let integrity = match author {
        AuthorRole::User => Integrity::UserDirect,
        _ => Integrity::Agent,
    };
    events.push(IncomingEvent {
        seq: seq(line, part),
        author_role: author,
        author_ref,
        occurred_at: at,
        content,
        idempotency_key: format!("{CLIENT}:{uuid}:{part}"),
        integrity,
        sensitivity: Sensitivity::Work,
    });
}

fn timestamp(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = r#"{"type":"custom-title","customTitle":"Orbit memory","sessionId":"s1"}
{"type":"user","uuid":"u1","sessionId":"s1","timestamp":"2026-11-12T10:00:00Z","entrypoint":"claude-desktop","cwd":"/p/orbit","origin":{"kind":"human"},"message":{"role":"user","content":"I prefer Rust for the engine"}}
{"type":"assistant","uuid":"a1","sessionId":"s1","timestamp":"2026-11-12T10:00:05Z","message":{"role":"assistant","model":"claude-x","content":[{"type":"thinking","thinking":""},{"type":"text","text":"Noted."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo test"}}]}}
{"type":"user","uuid":"u2","sessionId":"s1","timestamp":"2026-11-12T10:00:09Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
{"type":"user","uuid":"u3","sessionId":"s1","timestamp":"2026-11-12T10:01:00Z","isMeta":true,"message":{"role":"user","content":"injected reminder"}}
{"type":"user","uuid":"u4","sessionId":"s1","timestamp":"2026-11-12T10:02:00Z","isCompactSummary":true,"message":{"role":"user","content":"summary"}}
{"type":"mode","mode":"x","sessionId":"s1"}
not json"#;

    fn parsed() -> Import {
        parse(SESSION, &Options::default()).import.unwrap()
    }

    #[test]
    fn keeps_who_said_what() {
        let import = parsed();
        let roles: Vec<_> = import
            .events
            .iter()
            .map(|e| (e.author_role, e.integrity, e.content.as_str()))
            .collect();
        assert_eq!(
            roles,
            [
                (AuthorRole::User, Integrity::UserDirect, "I prefer Rust for the engine"),
                (AuthorRole::Assistant, Integrity::Agent, "Noted."),
                (AuthorRole::Assistant, Integrity::Agent, r#"{"command":"cargo test"}"#),
                (AuthorRole::Tool, Integrity::Agent, "ok"),
                (AuthorRole::System, Integrity::Agent, "injected reminder"),
            ]
        );
    }

    #[test]
    fn describes_the_source() {
        let s = parsed().source;
        assert_eq!(s.external_ref, "claude-code:s1");
        assert_eq!(s.title.as_deref(), Some("Orbit memory"));
        assert_eq!(s.app.as_deref(), Some("claude-desktop"));
        assert_eq!(s.model.as_deref(), Some("claude-x"));
        assert_eq!(s.meta["cwd"], "/p/orbit");
    }

    #[test]
    fn reports_what_it_left_out() {
        let k = parse(SESSION, &Options::default()).skipped;
        // The thinking block, the compact summary and the `mode` line.
        assert_eq!((k.unparsable, k.not_conversation), (1, 3));
    }

    #[test]
    fn keys_are_stable_and_ordered() {
        let i = parsed();
        assert_eq!(i.events[0].idempotency_key, "claude-code:u1:0");
        assert!(i.events.windows(2).all(|w| w[0].seq < w[1].seq));
        assert_eq!(parsed().events[2].idempotency_key, i.events[2].idempotency_key);
    }

    #[test]
    fn skips_tool_output_above_the_limit() {
        let p = parse(
            SESSION,
            &Options {
                max_tool_output_bytes: 1,
            },
        );
        assert_eq!(p.skipped.oversized, 1);
        assert!(
            p.import
                .unwrap()
                .events
                .iter()
                .all(|e| e.author_role != AuthorRole::Tool)
        );
    }
}
