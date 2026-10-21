//! Database connection setup, versioned schema migration, and optional dev seed.

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

use crate::migration::Migrator;

pub async fn connect(database_url: &str) -> anyhow::Result<DatabaseConnection> {
    let sqlite_memory = database_url.starts_with("sqlite:")
        && (database_url.contains(":memory:") || database_url.contains("mode=memory"));
    let mut options = ConnectOptions::new(database_url);
    options
        .max_connections(if sqlite_memory { 1 } else { 10 })
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(5))
        .statement_timeout(Duration::from_secs(5))
        .sqlx_logging(false)
        .record_stmt_in_spans(false);
    // The ORM's SQLite connector enables foreign keys by default. Keep one
    // connection for in-memory databases so all requests share the same schema.
    Ok(Database::connect(options).await?)
}

pub async fn migrate(db: &DatabaseConnection) -> anyhow::Result<()> {
    Migrator::up(db, None).await?;
    Ok(())
}

/// Insert the two demo learners (Alice, Bruno). Idempotent; runs when
/// compiled with `dev-seed` and enabled with `SEED_DEV_DATA=1`.
#[cfg(feature = "dev-seed")]
pub async fn seed_dev_data(db: &DatabaseConnection) -> anyhow::Result<()> {
    use chrono::Utc;
    use sea_orm::{EntityTrait, Set, sea_query::OnConflict};
    use uuid::Uuid;

    use crate::entities::{learner, lesson_progress};

    let alice_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111")?;
    let bruno_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222")?;
    let now = Utc::now();
    for (id, external_id, display_name, cefr_level, email, auth_subject, total_xp) in [
        (
            alice_id,
            "ext-alice",
            "Alice",
            "B1",
            "alice@example.com",
            "auth0|alice",
            0,
        ),
        (
            bruno_id,
            "ext-bruno",
            "Bruno",
            "A2",
            "bruno@example.com",
            "auth0|bruno",
            40,
        ),
    ] {
        learner::Entity::insert(learner::ActiveModel {
            id: Set(id),
            external_id: Set(external_id.to_owned()),
            display_name: Set(display_name.to_owned()),
            cefr_level: Set(cefr_level.to_owned()),
            email: Set(email.to_owned()),
            auth_subject: Set(auth_subject.to_owned()),
            total_xp: Set(total_xp),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::column(learner::Column::Id)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    }
    lesson_progress::Entity::insert(lesson_progress::ActiveModel {
        learner_id: Set(bruno_id),
        lesson_id: Set("lesson-greetings".to_owned()),
        completed_exercises: Set(4),
        xp: Set(40),
        updated_at: Set(now),
    })
    .on_conflict(
        OnConflict::columns([
            lesson_progress::Column::LearnerId,
            lesson_progress::Column::LessonId,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec_without_returning(db)
    .await?;
    Ok(())
}
