//! The Phase 0 path end to end: append, index, search, exact. Needs PostgreSQL.

use chrono::Utc;
use orbit_core::{AuthorRole, Integrity, Sensitivity};
use orbit_index::RawIndexer;
use orbit_ledger::{IncomingEvent, Ledger, SourceSpec, admit};
use orbit_recall::{ExactRequest, LiteralRole, Recall, SearchRequest, Verdict};
use sqlx::PgPool;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0b17);

async fn brain(pool: &PgPool, turns: &[(AuthorRole, &str)]) -> Recall {
    let ledger = Ledger::new(pool.clone(), TENANT);
    let source = ledger
        .upsert_source(&SourceSpec {
            kind: "conversation".into(),
            external_ref: "t-1".into(),
            app: Some("test".into()),
            model: None,
            title: Some("deploy chat".into()),
            meta: Default::default(),
        })
        .await
        .unwrap();
    let events: Vec<_> = turns
        .iter()
        .enumerate()
        .map(|(i, (author, text))| {
            admit(IncomingEvent {
                seq: i as i32,
                author_role: *author,
                author_ref: None,
                occurred_at: Utc::now(),
                content: (*text).into(),
                idempotency_key: format!("t-1:{i}"),
                integrity: if *author == AuthorRole::User {
                    Integrity::UserDirect
                } else {
                    Integrity::Agent
                },
                sensitivity: Sensitivity::Work,
            })
            .unwrap()
        })
        .collect();
    ledger.append_batch(source, &events).await.unwrap();
    while RawIndexer::new(pool.clone(), TENANT).run_once(100).await.unwrap() > 0 {}
    Recall::new(pool.clone(), TENANT, None)
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn finds_what_was_said_and_reports_when_nothing_matches(pool: PgPool) {
    let recall = brain(
        &pool,
        &[(
            AuthorRole::User,
            "We deploy the staging environment with blue-green releases",
        )],
    )
    .await;

    let hit = recall
        .search(&SearchRequest {
            query: "how do we deploy staging".into(),
            limit: 5,
            include_tool_activity: false,
        })
        .await
        .unwrap();
    assert_eq!(hit.status, Verdict::Grounded);
    assert_eq!(hit.hits[0].author, "user");

    let miss = recall
        .search(&SearchRequest {
            query: "kubernetes".into(),
            limit: 5,
            include_tool_activity: false,
        })
        .await
        .unwrap();
    assert_eq!(miss.status, Verdict::NotFound);
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn returns_exact_strings_byte_for_byte_with_who_typed_them(pool: PgPool) {
    let typed = "flyctl deploy --config fly.prod.toml --strategy bluegreen";
    let suggested = "flyctl deploy --config fly.staging.toml";
    let recall = brain(
        &pool,
        &[
            (AuthorRole::Assistant, &format!("You could run `{suggested}` first.")),
            (AuthorRole::User, &format!("I ran `{typed}` and it worked, NUL\0inside")),
        ],
    )
    .await;

    let result = recall
        .exact(&ExactRequest {
            what: "the deploy command".into(),
            kind: Some("command".into()),
            role: Some(LiteralRole::TypedByUser),
            limit: 5,
        })
        .await
        .unwrap();
    assert_eq!(result.status, Verdict::Grounded);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].exact, typed);
    assert_eq!(result.matches[0].role, "typed_by_user");

    let by_string = recall
        .exact(&ExactRequest {
            what: "fly.staging.toml".into(),
            kind: None,
            role: None,
            limit: 5,
        })
        .await
        .unwrap();
    assert!(
        by_string
            .matches
            .iter()
            .any(|m| m.exact == suggested && m.role == "suggested_by_assistant")
    );
}
