use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Existing installations retain their tables, constraints, and data. The
        // SeaORM migration ledger records adoption independently of SQLx's ledger.
        manager
            .create_table(
                Table::create()
                    .table(Learners::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Learners::Id).uuid().not_null().primary_key())
                    .col(
                        ColumnDef::new(Learners::ExternalId)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Learners::DisplayName).text().not_null())
                    .col(ColumnDef::new(Learners::CefrLevel).text().not_null())
                    .col(ColumnDef::new(Learners::Email).text().not_null())
                    .col(ColumnDef::new(Learners::AuthSubject).text().not_null())
                    .col(
                        ColumnDef::new(Learners::TotalXp)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Learners::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Learners::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .check(
                        Expr::col(Learners::CefrLevel).is_in(["A1", "A2", "B1", "B2", "C1", "C2"]),
                    )
                    .check(Expr::col(Learners::TotalXp).gte(0))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(LessonProgress::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(LessonProgress::LearnerId).uuid().not_null())
                    .col(ColumnDef::new(LessonProgress::LessonId).text().not_null())
                    .col(
                        ColumnDef::new(LessonProgress::CompletedExercises)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LessonProgress::Xp)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LessonProgress::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(LessonProgress::LearnerId)
                            .col(LessonProgress::LessonId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_lesson_progress_learner")
                            .from(LessonProgress::Table, LessonProgress::LearnerId)
                            .to(Learners::Table, Learners::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .check(Expr::col(LessonProgress::CompletedExercises).gte(0))
                    .check(Expr::col(LessonProgress::Xp).gte(0))
                    .to_owned(),
            )
            .await?;

        // SQLite only auto-increments an INTEGER primary key; PostgreSQL keeps
        // the original 64-bit sequence-backed identity.
        let mut event_id = ColumnDef::new(ProgressEvents::Id);
        if manager.get_database_backend() == sea_orm::DbBackend::Sqlite {
            event_id.integer();
        } else {
            event_id.big_integer();
        }
        event_id.not_null().auto_increment().primary_key();

        manager
            .create_table(
                Table::create()
                    .table(ProgressEvents::Table)
                    .if_not_exists()
                    .col(&mut event_id)
                    .col(ColumnDef::new(ProgressEvents::LearnerId).uuid().not_null())
                    .col(ColumnDef::new(ProgressEvents::LessonId).text().not_null())
                    .col(ColumnDef::new(ProgressEvents::ExerciseId).text().not_null())
                    .col(ColumnDef::new(ProgressEvents::EventType).text().not_null())
                    .col(
                        ColumnDef::new(ProgressEvents::XpAwarded)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProgressEvents::IdempotencyKey)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProgressEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_progress_events_learner_key")
                            .unique()
                            .col(ProgressEvents::LearnerId)
                            .col(ProgressEvents::IdempotencyKey),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_progress_events_learner")
                            .from(ProgressEvents::Table, ProgressEvents::LearnerId)
                            .to(Learners::Table, Learners::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .check(Expr::col(ProgressEvents::XpAwarded).gte(0))
                    .to_owned(),
            )
            .await?;

        for (name, sort_column, filtered) in [
            (
                "idx_events_learner_created_id",
                ProgressEvents::CreatedAt,
                false,
            ),
            ("idx_events_learner_xp_id", ProgressEvents::XpAwarded, false),
            (
                "idx_events_learner_type_created_id",
                ProgressEvents::CreatedAt,
                true,
            ),
            (
                "idx_events_learner_type_xp_id",
                ProgressEvents::XpAwarded,
                true,
            ),
        ] {
            let mut index = Index::create();
            index
                .name(name)
                .table(ProgressEvents::Table)
                .if_not_exists()
                .col(ProgressEvents::LearnerId);
            if filtered {
                index.col(ProgressEvents::EventType);
            }
            index
                .col((sort_column, IndexOrder::Desc))
                .col((ProgressEvents::Id, IndexOrder::Desc));
            manager.create_index(index.to_owned()).await?;
        }
        Ok(())
    }

    // Destructive even for tables adopted from the original SQLx schema.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ProgressEvents::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(LessonProgress::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Learners::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Learners {
    Table,
    Id,
    ExternalId,
    DisplayName,
    CefrLevel,
    Email,
    AuthSubject,
    TotalXp,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum LessonProgress {
    Table,
    LearnerId,
    LessonId,
    CompletedExercises,
    Xp,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ProgressEvents {
    Table,
    Id,
    LearnerId,
    LessonId,
    ExerciseId,
    EventType,
    XpAwarded,
    IdempotencyKey,
    CreatedAt,
}
