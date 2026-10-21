//! Lesson progress queries.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};
use uuid::Uuid;

use crate::{
    dto::{inputs::LessonId, lessons::LessonProgress},
    entities::lesson_progress,
    error::AppError,
};

pub async fn get_lesson_progress(
    db: &DatabaseConnection,
    learner_id: Uuid,
    lesson_id: &LessonId,
) -> Result<LessonProgress, AppError> {
    lesson_progress::Entity::find()
        .select_only()
        .columns([
            lesson_progress::Column::LessonId,
            lesson_progress::Column::CompletedExercises,
            lesson_progress::Column::Xp,
            lesson_progress::Column::UpdatedAt,
        ])
        .filter(lesson_progress::Column::LearnerId.eq(learner_id))
        .filter(lesson_progress::Column::LessonId.eq(lesson_id.as_ref()))
        .into_model::<LessonProgress>()
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}
