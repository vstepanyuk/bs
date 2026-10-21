use axum::Json;
use serde_json::{Value, json};

/// Liveness probe.
#[utoipa::path(
    get, path = "/healthz",
    responses((status = 200, description = "Process is alive", body = Object))
)]
pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
