//! `POST /v1/learners/{learner_id}/events` and
//! `GET  /v1/learners/{learner_id}/events`.

use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use uuid::Uuid;

use crate::{
    AppState,
    auth::Caller,
    dto::{
        events::{Event, EventQuery, RecordEventInput},
        inputs::IdempotencyKey,
    },
    error::AppError,
    repos::events as event_repo,
};

/// Record a completed-exercise event and roll it into the learner's totals.
///
/// The key identifies the original operation for this learner. Reusing it with
/// any valid payload returns the original event without applying new XP or
/// progress: the idempotency key wins, even when retry values differ.
#[utoipa::path(
    post,
    path = "/v1/learners/{learner_id}/events",
    tag = "events",
    params(
        ("learner_id" = Uuid, Path, description = "Learner UUID"),
        ("Idempotency-Key" = String, Header, description = "Required single nonblank printable ASCII key, scoped to this learner", min_length = 1, max_length = 200)
    ),
    request_body = RecordEventInput,
    responses(
        (status = 201, description = "Event created and learner totals updated", body = Event),
        (status = 200, description = "Replay: original event returned; the idempotency key wins over a different valid payload", body = Event),
        (status = 400, description = "Invalid learner UUID, JSON body, or Idempotency-Key"),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Caller cannot access this learner"),
        (status = 404, description = "Learner not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn record_event(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<RecordEventInput>, JsonRejection>,
) -> Result<(StatusCode, Json<Event>), AppError> {
    let Path(learner_id) = path.map_err(|_| AppError::validation("invalid learner id"))?;
    caller.require_learner_access(learner_id)?;
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .and_then(|key| IdempotencyKey::try_new(key).ok())
        .ok_or_else(|| {
            AppError::validation(
                "Idempotency-Key must be a nonblank printable ASCII value of at most 200 bytes",
            )
        })?;
    let Json(input) = body.map_err(|_| AppError::validation("invalid event JSON body"))?;

    let (event, created) = event_repo::record_event(&state.db, learner_id, key, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(event)))
}

/// List a learner's recorded events.
#[utoipa::path(
    get,
    path = "/v1/learners/{learner_id}/events",
    tag = "events",
    params(
        ("learner_id" = Uuid, Path, description = "Learner UUID"),
        EventQuery
    ),
    responses(
        (status = 200, description = "Learner events matching the filter and pagination", body = Vec<Event>),
        (status = 400, description = "Invalid learner UUID or query parameters; configured maximum page size also applies"),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Caller cannot access this learner"),
        (status = 404, description = "Learner not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_events(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<EventQuery>, QueryRejection>,
) -> Result<Json<Vec<Event>>, AppError> {
    let Path(learner_id) = path.map_err(|_| AppError::validation("invalid learner id"))?;
    caller.require_learner_access(learner_id)?;
    let Query(query) = query.map_err(|_| AppError::validation("invalid event query parameters"))?;
    query.validate(state.config.max_page_size)?;
    event_repo::list_events(&state.db, learner_id, &query)
        .await
        .map(Json)
}
