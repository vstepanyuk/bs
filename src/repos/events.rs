//! Transactional event persistence and queries.

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait, Order,
    QueryFilter, QueryOrder, QuerySelect, Select, Set,
    sea_query::{Expr, OnConflict},
};
use uuid::Uuid;

use crate::{
    dto::{
        events::{Event, EventQuery, EventSort, RecordEventInput, SortOrder},
        inputs::IdempotencyKey,
    },
    entities::{learner, lesson_progress, progress_event},
    error::AppError,
};

/// Returns the persisted event and whether it was newly created.
pub async fn record_event(
    db: &DatabaseConnection,
    learner_id: Uuid,
    idempotency_key: IdempotencyKey<'_>,
    input: RecordEventInput,
) -> Result<(Event, bool), AppError> {
    db.transaction_async(async move |transaction| {
        record_event_in_transaction(transaction, learner_id, idempotency_key, input).await
    })
    .await
    .map_err(AppError::from)
}

async fn record_event_in_transaction(
    transaction: &DatabaseTransaction,
    learner_id: Uuid,
    idempotency_key: IdempotencyKey<'_>,
    input: RecordEventInput,
) -> Result<(Event, bool), AppError> {
    let idempotency_key = idempotency_key.into_inner();
    // A no-op write acquires a PostgreSQL row lock or SQLite writer lock before
    // any reads. Every event for a learner uses this lock order, including retries.
    let locked = learner::Entity::update_many()
        .col_expr(
            learner::Column::UpdatedAt,
            Expr::col(learner::Column::UpdatedAt),
        )
        .filter(learner::Column::Id.eq(learner_id))
        .exec(transaction)
        .await?;
    if locked.rows_affected == 0 {
        return Err(AppError::NotFound);
    }

    if let Some(event) = public_events()
        .filter(progress_event::Column::LearnerId.eq(learner_id))
        .filter(progress_event::Column::IdempotencyKey.eq(idempotency_key))
        .into_model::<Event>()
        .one(transaction)
        .await?
    {
        // The original operation wins even when a retry has a different payload.
        return Ok((event, false));
    }

    let total_xp = learner::Entity::find_by_id(learner_id)
        .select_only()
        .column(learner::Column::TotalXp)
        .into_tuple::<i64>()
        .one(transaction)
        .await?
        .ok_or(AppError::NotFound)?;

    let now = Utc::now();
    let xp_awarded = input.xp_awarded.into_inner();
    let event = progress_event::ActiveModel {
        learner_id: Set(learner_id),
        lesson_id: Set(input.lesson_id.into_inner()),
        exercise_id: Set(input.exercise_id.into_inner()),
        event_type: Set(input.event_type.into_inner()),
        xp_awarded: Set(xp_awarded),
        idempotency_key: Set(idempotency_key.to_owned()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(transaction)
    .await?;

    let progress = lesson_progress::Entity::find()
        .select_only()
        .columns([
            lesson_progress::Column::CompletedExercises,
            lesson_progress::Column::Xp,
        ])
        .filter(lesson_progress::Column::LearnerId.eq(learner_id))
        .filter(lesson_progress::Column::LessonId.eq(event.lesson_id.as_str()))
        .into_tuple::<(i32, i64)>()
        .one(transaction)
        .await?;

    // SQLite can promote overflowing integer arithmetic to REAL. Check every
    // counter in Rust so both backends fail and roll back the same operation.
    let xp_awarded = i64::from(xp_awarded);
    let total_xp = total_xp
        .checked_add(xp_awarded)
        .ok_or_else(counter_overflow)?;
    let (completed_exercises, lesson_xp) = match progress {
        Some((completed_exercises, xp)) => (
            completed_exercises
                .checked_add(1)
                .ok_or_else(counter_overflow)?,
            xp.checked_add(xp_awarded).ok_or_else(counter_overflow)?,
        ),
        None => (1, xp_awarded),
    };

    learner::Entity::update_many()
        .col_expr(learner::Column::TotalXp, Expr::val(total_xp))
        .col_expr(learner::Column::UpdatedAt, Expr::val(now))
        .filter(learner::Column::Id.eq(learner_id))
        .exec(transaction)
        .await?;

    lesson_progress::Entity::insert(lesson_progress::ActiveModel {
        learner_id: Set(learner_id),
        lesson_id: Set(event.lesson_id.clone()),
        completed_exercises: Set(completed_exercises),
        xp: Set(lesson_xp),
        updated_at: Set(now),
    })
    .on_conflict(
        OnConflict::columns([
            lesson_progress::Column::LearnerId,
            lesson_progress::Column::LessonId,
        ])
        .update_columns([
            lesson_progress::Column::CompletedExercises,
            lesson_progress::Column::Xp,
            lesson_progress::Column::UpdatedAt,
        ])
        .to_owned(),
    )
    .exec(transaction)
    .await?;

    Ok((
        Event {
            id: event.id,
            lesson_id: event.lesson_id,
            exercise_id: event.exercise_id,
            event_type: event.event_type,
            xp_awarded: event.xp_awarded,
            created_at: event.created_at,
        },
        true,
    ))
}

fn counter_overflow() -> AppError {
    AppError::Internal(anyhow::anyhow!("progress counter overflow"))
}

fn public_events() -> Select<progress_event::Entity> {
    progress_event::Entity::find().select_only().columns([
        progress_event::Column::Id,
        progress_event::Column::LessonId,
        progress_event::Column::ExerciseId,
        progress_event::Column::EventType,
        progress_event::Column::XpAwarded,
        progress_event::Column::CreatedAt,
    ])
}

pub async fn list_events(
    db: &DatabaseConnection,
    learner_id: Uuid,
    query: &EventQuery,
) -> Result<Vec<Event>, AppError> {
    if !super::learners::learner_exists(db, learner_id).await? {
        return Err(AppError::NotFound);
    }

    let sort = match query.sort {
        EventSort::CreatedAt => progress_event::Column::CreatedAt,
        EventSort::XpAwarded => progress_event::Column::XpAwarded,
    };
    let order = match query.order {
        SortOrder::Asc => Order::Asc,
        SortOrder::Desc => Order::Desc,
    };
    let mut select = public_events().filter(progress_event::Column::LearnerId.eq(learner_id));
    if let Some(event_type) = &query.event_type {
        select = select.filter(progress_event::Column::EventType.eq(event_type.as_ref()));
    }
    Ok(select
        .order_by(sort, order.clone())
        .order_by(progress_event::Column::Id, order)
        .limit(u64::from(query.limit.into_inner()))
        .offset(u64::from(query.offset.into_inner()))
        .into_model::<Event>()
        .all(db)
        .await?)
}
