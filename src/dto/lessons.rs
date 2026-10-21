//! Public lesson progress responses.

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, sea_orm::FromQueryResult, utoipa::ToSchema)]
pub struct LessonProgress {
    pub lesson_id: String,
    pub completed_exercises: i32,
    pub xp: i64,
    pub updated_at: DateTime<Utc>,
}
