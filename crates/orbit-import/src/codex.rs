//! Codex session files (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`).
//!
//! Uses the `item_completed` events: typed user messages, agent messages and
//! executed commands. Reasoning is encrypted and other items are not conversation.

use crate::{Import, Options, Parsed, Skipped, seq};
use chrono::{DateTime, Utc};
use orbit_core::{AuthorRole, Integrity, Sensitivity};
use orbit_ledger::{IncomingEvent, SourceSpec};
use serde_json::{Map, Value};

const CLIENT: &str = "codex";

/// Parses one session file. `import` is `None` when it holds no conversation.
pub fn parse(text: &str, options: &Options) -> Parsed {
    let mut events = Vec::new();
    let mut skipped = Skipped::default();
    let (mut session, mut app, mut model) = (None, None, None);
    let mut meta = Map::new();

    for (n, line) in text.lines().enumerate() {
        let Ok(o) = serde_json::from_str::<Value>(line) else {
            skipped.unparsable += 1;
            continue;
        };
        let payload = &o["payload"];
        match (o["type"].as_str(), payload["type"].as_str()) {
            (Some("session_meta"), _) => {
                session = payload["id"].as_str().map(str::to_string);
                app = payload["originator"].as_str().map(str::to_string);
                if let Some(cwd) = payload["cwd"].as_str() {
                    meta.insert("cwd".into(), Value::String(cwd.into()));
                }
            }
            (Some("turn_context"), _) => {
                if let Some(m) = payload["model"].as_str() {
                    model = Some(m.to_string());
                }
            }
            (Some("event_msg"), Some("item_completed")) => {
                let Some(at) = timestamp(&o["timestamp"]) else {
                    skipped.unparsable += 1;
                    continue;
                };
                let Some(session) = session.as_deref() else {
                    skipped.unparsable += 1;
                    continue;
                };
                item(
                    &payload["item"],
                    session,
                    n,
                    at,
                    model.as_deref(),
                    options,
                    &mut events,
                    &mut skipped,
                );
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
            title: None,
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
fn item(
    item: &Value,
    session: &str,
    n: usize,
    at: DateTime<Utc>,
    model: Option<&str>,
    options: &Options,
    events: &mut Vec<IncomingEvent>,
    skipped: &mut Skipped,
) {
    let id = item["id"].as_str().unwrap_or("");
    let mut add = |part: usize, author: AuthorRole, author_ref: Option<String>, content: String| {
        if content.trim().is_empty() {
            return;
        }
        let integrity = if author == AuthorRole::User {
            Integrity::UserDirect
        } else {
            Integrity::Agent
        };
        events.push(IncomingEvent {
            seq: seq(n, part),
            author_role: author,
            author_ref,
            occurred_at: at,
            content,
            idempotency_key: format!("{CLIENT}:{session}:{id}:{part}"),
            integrity,
            sensitivity: Sensitivity::Work,
        });
    };

    match item["type"].as_str() {
        Some("UserMessage") => {
            // Codex puts client context (file lists, tool results, guardian notes) into the
            // same item as what the person typed, with no flag telling them apart. Only the
            // first text block, when it carries no client marker, is taken as typed; the rest
            // is stored as client context, at lower trust.
            for (i, b) in item["content"].as_array().into_iter().flatten().enumerate() {
                if let Some(text) = b["text"].as_str() {
                    let typed = i == 0 && !client_marker(text);
                    let author = if typed { AuthorRole::User } else { AuthorRole::System };
                    add(
                        i,
                        author,
                        (!typed).then(|| "client_context".to_string()),
                        text.to_string(),
                    );
                }
            }
        }
        Some("AgentMessage") => {
            for (i, b) in item["content"].as_array().into_iter().flatten().enumerate() {
                if let Some(text) = b["text"].as_str() {
                    add(i, AuthorRole::Assistant, model.map(str::to_string), text.to_string());
                }
            }
        }
        Some("CommandExecution") => {
            let command = match &item["command"] {
                Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            add(0, AuthorRole::Assistant, Some("command_execution".into()), command);
            let output = item["aggregated_output"].as_str().unwrap_or("");
            if output.len() > options.max_tool_output_bytes {
                skipped.oversized += 1;
            } else {
                add(
                    1,
                    AuthorRole::Tool,
                    Some("command_execution".into()),
                    output.to_string(),
                );
            }
        }
        _ => skipped.not_conversation += 1,
    }
}

/// Text the Codex client adds itself: tagged context, file and agent listings, tool results.
fn client_marker(text: &str) -> bool {
    let t = text.trim_start();
    let tool_result = t
        .strip_prefix('[')
        .and_then(|r| r.split_once("] tool"))
        .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()));
    t.starts_with('<')
        || t.starts_with("# Files")
        || t.starts_with("# AGENTS")
        || t.starts_with("# Context")
        || tool_result
}

fn timestamp(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = r#"{"timestamp":"2026-11-12T09:00:00Z","type":"session_meta","payload":{"id":"th1","originator":"codex_desktop","cwd":"/p/orbit"}}
{"timestamp":"2026-11-12T09:00:01Z","type":"turn_context","payload":{"model":"gpt-x","cwd":"/p/orbit"}}
{"timestamp":"2026-11-12T09:00:02Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","id":"i1","content":[{"type":"text","text":"Use pnpm here","text_elements":[]},{"type":"text","text":"[3] tool js result: ok","text_elements":[]}]}}}
{"timestamp":"2026-11-12T09:00:02Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","id":"i0","content":[{"type":"text","text":"<environment_context>cwd</environment_context>","text_elements":[]}]}}}
{"timestamp":"2026-11-12T09:00:03Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"Reasoning","id":"i2"}}}
{"timestamp":"2026-11-12T09:00:04Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"i3","command":"pnpm install","aggregated_output":"done","exit_code":0}}}
{"timestamp":"2026-11-12T09:00:05Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","id":"i4","content":[{"type":"Text","text":"Installed."}]}}}
{"timestamp":"2026-11-12T09:00:06Z","type":"token_usage_record","payload":{}}"#;

    #[test]
    fn keeps_typed_messages_commands_and_replies() {
        let parsed = parse(SESSION, &Options::default());
        let i = parsed.import.unwrap();
        let got: Vec<_> = i
            .events
            .iter()
            .map(|e| (e.author_role, e.author_ref.as_deref(), e.content.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (AuthorRole::User, None, "Use pnpm here"),
                (AuthorRole::System, Some("client_context"), "[3] tool js result: ok"),
                (
                    AuthorRole::System,
                    Some("client_context"),
                    "<environment_context>cwd</environment_context>"
                ),
                (AuthorRole::Assistant, Some("command_execution"), "pnpm install"),
                (AuthorRole::Tool, Some("command_execution"), "done"),
                (AuthorRole::Assistant, Some("gpt-x"), "Installed."),
            ]
        );
        assert_eq!(i.events[0].integrity, Integrity::UserDirect);
        assert_eq!(i.source.external_ref, "codex:th1");
        assert_eq!(parsed.skipped.not_conversation, 2);
    }
}
