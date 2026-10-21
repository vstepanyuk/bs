use axum::{Json, Router, extract::DefaultBodyLimit, routing::get};
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::AppState;

mod events;
mod health;
mod learners;
mod lessons;

fn documented_router() -> OpenApiRouter<AppState> {
    let api = utoipa::openapi::OpenApi::new(
        utoipa::openapi::Info::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        utoipa::openapi::path::Paths::new(),
    );
    let mut router = OpenApiRouter::with_openapi(api)
        .routes(routes!(health::healthz))
        .routes(routes!(learners::get_learner, learners::patch_learner))
        .routes(routes!(lessons::get_lesson_progress))
        .routes(routes!(events::record_event, events::list_events));
    router
        .get_openapi_mut()
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );
    router
}

/// Generated from the same handler registrations used by the live router.
pub fn openapi() -> utoipa::openapi::OpenApi {
    documented_router().into_openapi()
}

pub fn router(state: AppState) -> Router {
    let (router, api) = documented_router().split_for_parts();
    #[cfg(all(feature = "dev-docs", debug_assertions))]
    let router = router.merge(
        utoipa_swagger_ui::SwaggerUi::new("/docs")
            .config(utoipa_swagger_ui::Config::from("/openapi.json")),
    );
    router
        .route("/openapi.json", get(move || async move { Json(api) }))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(state)
}
