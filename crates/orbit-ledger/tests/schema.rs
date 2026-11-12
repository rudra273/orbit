//! Database invariants. Needs PostgreSQL: `make db`, then `make test-db`.

use orbit_core::{AuthorRole, Integrity, Sensitivity};
use orbit_ledger::{IncomingEvent, Ledger, SourceSpec, admit};
use sqlx::PgPool;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0b17);

async fn one_evidence(pool: &PgPool) -> Uuid {
    let ledger = Ledger::new(pool.clone(), TENANT);
    let source = ledger
        .upsert_source(&SourceSpec {
            kind: "conversation".into(),
            external_ref: "t-1".into(),
            app: None,
            model: None,
            title: None,
            meta: Default::default(),
        })
        .await
        .unwrap();
    let event = admit(IncomingEvent {
        seq: 0,
        author_role: AuthorRole::User,
        author_ref: None,
        occurred_at: chrono::Utc::now(),
        content: "I prefer Rust".into(),
        idempotency_key: "t-1:0".into(),
        integrity: Integrity::UserDirect,
        sensitivity: Sensitivity::Work,
    })
    .unwrap();
    ledger.append(source, &event).await.unwrap().evidence_id.0
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn evidence_cannot_be_edited_or_deleted(pool: PgPool) {
    let id = one_evidence(&pool).await;
    let edit = sqlx::query("update evidence set content = 'I prefer Go' where evidence_id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(edit.is_err());
    let delete = sqlx::query("delete from evidence where evidence_id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(delete.is_err());
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn evidence_content_can_be_erased(pool: PgPool) {
    let id = one_evidence(&pool).await;
    sqlx::query("update evidence set content = null, erased_at = now() where evidence_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}

async fn insert_belief(
    pool: &PgPool,
    derivation: Uuid,
    subject: Uuid,
    lifecycle: &str,
    standing: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into belief (tenant_id, slot_key, version_key, kind, subject_id, predicate, scope_key, statement_tpl,
                             valid, basis, standing, lifecycle, confidence, integrity, sensitivity, derivation_id)
         values ($1, 'slot', $2, 'identity.preference', $3, 'preferred_language', '{}', 'Prefers a language',
                 tstzrange('2026-01-01', null), 'stated', $4::standing, $5::lifecycle, 0.9, 'user_direct', 'work', $6)",
    )
    .bind(TENANT)
    .bind(Uuid::now_v7())
    .bind(subject)
    .bind(standing)
    .bind(lifecycle)
    .bind(derivation)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test(migrator = "orbit_ledger::MIGRATOR")]
#[ignore = "needs PostgreSQL"]
async fn a_slot_has_one_current_belief_unless_disputed(pool: PgPool) {
    let derivation: Uuid = sqlx::query_scalar(
        "insert into derivation (stage, engine_version) values ('revise', 'test') returning derivation_id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let subject: Uuid = sqlx::query_scalar(
        "insert into entity (tenant_id, type, canonical_name, cluster_id) values ($1, 'person', 'me', uuidv7()) returning entity_id",
    )
    .bind(TENANT)
    .fetch_one(&pool)
    .await
    .unwrap();

    insert_belief(&pool, derivation, subject, "current", "unconfirmed")
        .await
        .unwrap();
    assert!(
        insert_belief(&pool, derivation, subject, "current", "unconfirmed")
            .await
            .is_err(),
        "two current versions"
    );
    insert_belief(&pool, derivation, subject, "superseded", "unconfirmed")
        .await
        .unwrap();
    insert_belief(&pool, derivation, subject, "current", "disputed")
        .await
        .unwrap();
}
