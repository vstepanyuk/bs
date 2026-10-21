//! Event input, public responses, query parameters, and validation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::inputs::{
    EventTypeFilter, ExerciseEventType, ExerciseId, LessonId, PageLimit, PageOffset, XpAwarded,
};
use crate::error::AppError;

/// An event's public representation; excludes learner and idempotency data.
#[derive(Serialize, sea_orm::FromQueryResult, utoipa::ToSchema)]
pub struct Event {
    pub id: i64,
    pub lesson_id: String,
    pub exercise_id: String,
    pub event_type: String,
    pub xp_awarded: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordEventInput {
    #[schema(value_type = String, min_length = 1, max_length = 200)]
    pub(crate) lesson_id: LessonId,
    #[schema(value_type = String, min_length = 1, max_length = 200)]
    pub(crate) exercise_id: ExerciseId,
    #[schema(value_type = String, pattern = "^exercise_completed$")]
    pub(crate) event_type: ExerciseEventType,
    #[schema(value_type = i32, minimum = 0)]
    pub(crate) xp_awarded: XpAwarded,
}

#[derive(Clone, Copy, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventSort {
    #[default]
    CreatedAt,
    XpAwarded,
}

#[derive(Clone, Copy, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SortOrder {
    Asc,
    #[default]
    Desc,
}

#[derive(Deserialize, utoipa::IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct EventQuery {
    #[serde(default)]
    #[param(value_type = i64, minimum = 1, maximum = 100, default = 20, required = false)]
    pub(crate) limit: PageLimit,
    #[serde(default)]
    #[param(value_type = i64, minimum = 0, maximum = 10000, default = 0, required = false)]
    pub(crate) offset: PageOffset,
    #[serde(default)]
    /// Sort field; defaults to created_at.
    #[param(inline, required = false)]
    pub(crate) sort: EventSort,
    #[serde(default)]
    /// Sort direction; defaults to desc.
    #[param(inline, required = false)]
    pub(crate) order: SortOrder,
    #[param(value_type = Option<String>, min_length = 1, max_length = 100)]
    pub(crate) event_type: Option<EventTypeFilter>,
}

impl EventQuery {
    pub fn validate(&self, max_page_size: i64) -> Result<(), AppError> {
        if i64::from(self.limit.into_inner()) > max_page_size {
            return Err(AppError::validation("limit exceeds the maximum page size"));
        }
        Ok(())
    }
}
