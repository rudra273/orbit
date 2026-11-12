//! The dense channel end to end: append, embed, search, erase, swap spaces. Needs PostgreSQL.
//! Uses the offline word-hash embedder, so these tests check the plumbing, not retrieval quality
//! (that is evals/embed-bench).

use chrono::Utc;
use orbit_core::{AuthorRole, Integrity, Sensitivity};
use orbit_embed::{
    Embedder, Space,
    testing::{Down, WordHash},
};
use orbit_index::{DenseIndexer, RawIndexer, SpaceState};
use orbit_ledger::{IncomingEvent, Ledger, SourceSpec, admit};
use orbit_recall::{Channel, Recall, SearchRequest, Verdict};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0xde75e);
const LONG_TAIL: &str = "The migration plan moves the ledger to PostgreSQL eighteen with uuidv7 keys.";

struct Turn {
    author: AuthorRole,
    author_ref: Option<&'static str>,
    sensitivity: Sensitivity,
    text: String,
}

fn said(author: AuthorRole, text: &str) -> Turn {
    Turn {
        author,
        author_ref: None,
        sensitivity: Sensitivity::Work,
        text: text.to_string(),
    }
}

async fn append(pool: &PgPool, conversation: &str, turns: &[Turn]) {
    let ledger = Ledger::new(pool.clone(), TENANT);
    let source = ledger
        .upsert_source(&SourceSpec {
            kind: "conversation".into(),
            external_ref: conversation.into(),
            app: Some("test".into()),
            model: None,
            title: Some(conversation.into()),
            meta: Default::default(),
        })
        .await
        .unwrap();
    let events: Vec<_> = turns
        .iter()
        .enumerate()
        .map(|(i, t)| {
            admit(IncomingEvent {
                seq: i as i32,
                author_role: t.author,
                author_ref: t.author_ref.map(String::from),
                occurred_at: Utc::now(),
                content: t.text.clone(),
                idempotency_key: format!("{conversation}:{i}"),
                integrity: if t.author == AuthorRole::User {
                    Integrity::UserDirect
                } else {
                    Integrity::Agent
                },
                sensitivity: t.sensitivity,
            })
            .unwrap()
        })
        .collect();
    ledger.append_batch(source, &events).await.unwrap();
}

fn space(dims: usize) -> Space {
    Space::hosted("test/word-hash", dims)
}

async fn index(pool: &PgPool, space: &Space) -> DenseIndexer {
    while RawIndexer::new(pool.clone(), TENANT).run_once(100).await.unwrap() > 0 {}
    let dense = DenseIndexer::new(pool.clone(), TENANT, space.clone(), Arc::new(WordHash));
    dense.register().await.unwrap();
    while dense.run_once(100).await.unwrap() > 0 {}
    dense
}

fn ask(query: &str) -> SearchRequest {
    SearchRequest {
        query: query.into(),
        limit: 5,
        include_tool_activity: false,
    }
}

async fn embedded(pool: &PgPool, space: &Space) -> Vec<(String, i32)> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("select set_config('orbit.tenant_id', $1, true)")
        .bind(TENANT.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query_as(
        "select convert_from(e.content, 'UTF8'), x.chunk from evidence_embedding x join evidence e using (tenant_id, evidence_id)
         where x.space_id = $1 order by e.seq, x.chunk",
    )
    .bind(&space.id)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn embeds_only_conversation_text_that_may_leave_the_server(pool: PgPool) {
    let long = format!(
        "{}\n\n{LONG_TAIL}",
        "Background on the project and its many parts. ".repeat(40)
    );
    append(
        &pool,
        "c-1",
        &[
            said(
                AuthorRole::User,
                "We deploy the staging environment with blue-green releases every Friday",
            ),
            said(AuthorRole::Assistant, &long),
            said(AuthorRole::User, "ok do it"),
            Turn {
                author: AuthorRole::User,
                author_ref: None,
                sensitivity: Sensitivity::Sensitive,
                text: "My bank account recovery phrase lives in the blue notebook on the shelf".into(),
            },
            Turn {
                author: AuthorRole::Assistant,
                author_ref: Some("tool_use:Bash"),
                sensitivity: Sensitivity::Work,
                text: r#"{"command":"cargo test --workspace --all-targets"}"#.into(),
            },
            said(
                AuthorRole::Tool,
                "test result: ok. 42 passed; 0 failed; 0 ignored; finished in 0.3s",
            ),
        ],
    )
    .await;
    let space = space(256);
    index(&pool, &space).await;

    let rows = embedded(&pool, &space).await;
    let texts: Vec<&str> = rows.iter().map(|(t, _)| t.as_str()).collect();
    assert!(texts[0].starts_with("We deploy the staging"));
    assert!(
        texts[1..].iter().all(|t| *t == long),
        "the long reply is cut into chunks: {texts:?}"
    );
    assert!(rows.len() >= 3, "two or more chunks for the long reply");
    assert!(
        !texts.iter().any(|t| t.contains("recovery phrase")),
        "sensitive text is never sent to a hosted model"
    );
    assert!(
        !texts
            .iter()
            .any(|t| t.contains("cargo test") || t.contains("42 passed") || *t == "ok do it")
    );
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn searches_by_meaning_and_returns_the_passage_that_matched(pool: PgPool) {
    // 1,564 characters of background, then a paragraph break inside the first 1,600: two chunks.
    let long = format!(
        "{}\n\n{LONG_TAIL}",
        "Background on the project and its many parts. ".repeat(34)
    );
    append(&pool, "c-1", &[said(AuthorRole::Assistant, &long)]).await;
    let space = space(256);
    index(&pool, &space).await;
    let embedder: Arc<dyn Embedder> = Arc::new(WordHash);
    let recall = Recall::new(pool.clone(), TENANT, Some(embedder));

    let result = recall
        .search(&ask("migration plan for the ledger uuidv7 keys"))
        .await
        .unwrap();
    assert_eq!(result.channels, vec![Channel::Dense, Channel::Lexical]);
    assert_eq!(result.status, Verdict::Grounded);
    let hit = &result.hits[0];
    assert_eq!(hit.text, LONG_TAIL, "the matching chunk, not the start of the reply");
    assert!(hit.truncated);
    assert_eq!(hit.matched_by, vec![Channel::Dense, Channel::Lexical]);

    // Only stop words: full-text search has nothing to match, so hits are by meaning alone.
    let vague = recall.search(&ask("the and its")).await.unwrap();
    assert_eq!(vague.status, Verdict::Partial);
    assert!(vague.hits.iter().all(|h| h.matched_by == vec![Channel::Dense]));
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn falls_back_to_words_when_the_provider_is_down(pool: PgPool) {
    append(
        &pool,
        "c-1",
        &[said(
            AuthorRole::User,
            "We deploy the staging environment with blue-green releases",
        )],
    )
    .await;
    let space = space(64);
    index(&pool, &space).await;
    let embedder: Arc<dyn Embedder> = Arc::new(Down);
    let recall = Recall::new(pool.clone(), TENANT, Some(embedder));

    let result = recall.search(&ask("deploy staging")).await.unwrap();
    assert_eq!(result.channels, vec![Channel::Lexical]);
    assert_eq!(result.status, Verdict::Grounded);
    assert_eq!(result.hits[0].matched_by, vec![Channel::Lexical]);

    let failing = DenseIndexer::new(pool.clone(), TENANT, space, Arc::new(Down));
    append(
        &pool,
        "c-2",
        &[said(
            AuthorRole::User,
            "A second message long enough to need embedding again",
        )],
    )
    .await;
    assert!(
        failing.run_once(100).await.is_err(),
        "the cursor does not move past text it could not embed"
    );
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn erasing_evidence_removes_its_vectors(pool: PgPool) {
    append(
        &pool,
        "c-1",
        &[said(
            AuthorRole::User,
            "We deploy the staging environment with blue-green releases",
        )],
    )
    .await;
    let space = space(64);
    index(&pool, &space).await;
    assert_eq!(embedded(&pool, &space).await.len(), 1);

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("select set_config('orbit.tenant_id', $1, true)")
        .bind(TENANT.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("update evidence set erased_at = now(), content = null")
        .execute(&mut *tx)
        .await
        .unwrap();
    let left: i64 =
        sqlx::query_scalar("select (select count(*) from evidence_embedding) + (select count(*) from evidence_search)")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn a_new_space_takes_over_only_after_catching_up(pool: PgPool) {
    append(
        &pool,
        "c-1",
        &[said(
            AuthorRole::User,
            "We deploy the staging environment with blue-green releases",
        )],
    )
    .await;
    let old = space(64);
    index(&pool, &old).await;

    let new = DenseIndexer::new(pool.clone(), TENANT, space(128), Arc::new(WordHash));
    assert_eq!(
        new.register().await.unwrap(),
        SpaceState::Building,
        "the old space keeps serving"
    );
    while new.run_once(100).await.unwrap() > 0 {}
    new.run_once(100).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("select set_config('orbit.tenant_id', $1, true)")
        .bind(TENANT.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let states: Vec<(String, String)> = sqlx::query_as("select space_id, state from embedding_space order by dims")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        states,
        vec![
            ("test/word-hash@64".into(), "retired".into()),
            ("test/word-hash@128".into(), "active".into())
        ]
    );

    let changed = DenseIndexer::new(
        pool.clone(),
        TENANT,
        Space {
            query_prefix: "q: ".into(),
            ..space(128)
        },
        Arc::new(WordHash),
    );
    assert!(changed.register().await.is_err(), "a space's settings never change");
}
