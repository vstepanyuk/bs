//! `GET /v1/learners/{learner_id}/lessons/{lesson_id}`.

use axum::{
    Json,
    extract::{Path, State, rejection::PathRejection},
};
use uuid::Uuid;

use crate::{
    AppState,
    auth::Caller,
    dto::{inputs::LessonId, lessons::LessonProgress},
    error::AppError,
    repos::lessons,
};

#[utoipa::path(
    get,
    path = "/v1/learners/{learner_id}/lessons/{lesson_id}",
    params(
        ("learner_id" = Uuid, Path),
        ("lesson_id" = String, Path, min_length = 1, max_length = 200)
    ),
    responses(
        (status = 200, description = "Lesson progress", body = LessonProgress),
        (status = 400, description = "Invalid path"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Progress not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_lesson_progress(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<(Uuid, LessonId)>, PathRejection>,
) -> Result<Json<LessonProgress>, AppError> {
    let Path((learner_id, lesson_id)) =
        path.map_err(|_| AppError::validation("invalid learner or lesson id"))?;
    caller.require_learner_access(learner_id)?;
    lessons::get_lesson_progress(&state.db, learner_id, &lesson_id)
        .await
        .map(Json)
}
