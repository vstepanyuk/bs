//! `GET /v1/learners/{learner_id}` and `PATCH /v1/learners/{learner_id}`.

use axum::{
    Json,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
};
use uuid::Uuid;

use crate::{
    AppState,
    auth::Caller,
    dto::learners::{LearnerProfile, PatchLearner},
    error::AppError,
    repos::learners,
};

/// Return the learner's public profile.
///
/// Learners may read only their own profile; service callers may read any learner.
/// The response excludes private identity fields.
#[utoipa::path(
    get,
    path = "/v1/learners/{learner_id}",
    tag = "learners",
    params(("learner_id" = Uuid, Path, description = "Learner UUID")),
    responses(
        (status = 200, description = "Public learner profile", body = LearnerProfile),
        (status = 400, description = "Invalid learner UUID"),
        (status = 401, description = "Missing or invalid bearer credentials"),
        (status = 403, description = "Caller cannot access this learner"),
        (status = 404, description = "Learner not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_learner(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<LearnerProfile>, AppError> {
    let Path(learner_id) = path.map_err(|_| AppError::validation("invalid learner id"))?;
    caller.require_learner_access(learner_id)?;
    learners::get_learner(&state.db, learner_id).await.map(Json)
}

/// Update the learner's mutable profile fields.
///
/// Learners may update only their own profile; service callers may update any learner.
/// Only display_name and cefr_level are mutable. Omitted fields remain unchanged;
/// an empty object is accepted. Null values and unknown fields are rejected.
/// Display names must be nonblank, contain 1 to 200 characters and contain no NUL.
#[utoipa::path(
    patch,
    path = "/v1/learners/{learner_id}",
    tag = "learners",
    params(("learner_id" = Uuid, Path, description = "Learner UUID")),
    request_body = PatchLearner,
    responses(
        (status = 200, description = "Updated public learner profile", body = LearnerProfile),
        (status = 400, description = "Invalid learner UUID, JSON body, display name or CEFR level; null and unknown fields are rejected"),
        (status = 401, description = "Missing or invalid bearer credentials"),
        (status = 403, description = "Caller cannot access this learner"),
        (status = 404, description = "Learner not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn patch_learner(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<PatchLearner>, JsonRejection>,
) -> Result<Json<LearnerProfile>, AppError> {
    let Path(learner_id) = path.map_err(|_| AppError::validation("invalid learner id"))?;
    caller.require_learner_access(learner_id)?;
    let Json(patch) = body.map_err(|_| AppError::validation("invalid learner patch body"))?;
    learners::patch_learner(&state.db, learner_id, &patch)
        .await
        .map(Json)
}
