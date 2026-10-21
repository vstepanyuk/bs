use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use learner_progress_service::{
    AppState, build_router,
    config::{Config, SecretString},
    db,
};
use tower::ServiceExt;

#[tokio::test]
async fn swagger_ui_is_available_only_in_opted_in_debug_builds() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = build_router(AppState::new(
        pool,
        Config {
            database_url: SecretString::new("unused"),
            bind_addr: "127.0.0.1:0".into(),
            content_service_api_key: SecretString::new("unused"),
            max_page_size: 100,
        },
    ));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/docs/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let expected = if cfg!(all(feature = "dev-docs", debug_assertions)) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    assert_eq!(response.status(), expected);
}
