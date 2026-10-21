use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use learner_progress_service::{
    AppState, build_router,
    config::{Config, SecretString},
    db,
    entities::{learner, lesson_progress, progress_event},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

fn app(pool: DatabaseConnection) -> Router {
    app_with_page_limit(pool, 100)
}

fn app_with_page_limit(pool: DatabaseConnection, max_page_size: i64) -> Router {
    build_router(AppState::new(
        pool,
        Config {
            database_url: SecretString::new("unused"),
            bind_addr: "127.0.0.1:0".into(),
            content_service_api_key: SecretString::new("secret"),
            max_page_size,
        },
    ))
}

async fn database() -> DatabaseConnection {
    let db = db::connect("sqlite::memory:").await.unwrap();
    db::migrate(&db).await.unwrap();
    db
}

#[tokio::test]
async fn migration_down_removes_populated_schema_and_allows_reapplying() {
    use learner_progress_service::migration::Migrator;
    use sea_orm_migration::{MigratorTrait, SchemaManager};

    let pool = database().await;
    let id = seed(&pool).await;
    let response = request(
        &app(pool.clone()),
        "POST",
        &format!("/v1/learners/{id}/events"),
        Some("service:test"),
        Some("rollback-schema"),
        event(7),
    )
    .await;
    assert_eq!(response.0, StatusCode::CREATED);

    Migrator::down(&pool, Some(1)).await.unwrap();
    let manager = SchemaManager::new(&pool);
    for table in ["progress_events", "lesson_progress", "learners"] {
        assert!(!manager.has_table(table).await.unwrap());
    }

    db::migrate(&pool).await.unwrap();
    assert_eq!(learner::Entity::find().count(&pool).await.unwrap(), 0);
    let id = seed(&pool).await;
    let response = request(
        &app(pool),
        "POST",
        &format!("/v1/learners/{id}/events"),
        Some("service:test"),
        Some("rollback-schema"),
        event(7),
    )
    .await;
    assert_eq!(response.0, StatusCode::CREATED);
}

async fn seed(pool: &DatabaseConnection) -> Uuid {
    let id = Uuid::new_v4();
    let now = Utc::now();
    learner::ActiveModel {
        id: Set(id),
        external_id: Set(id.to_string()),
        display_name: Set("Alice".into()),
        cefr_level: Set("B1".into()),
        email: Set("private@example.com".into()),
        auth_subject: Set("private-subject".into()),
        total_xp: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(pool)
    .await
    .unwrap();
    id
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    key: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(key) = key {
        builder = builder.header("idempotency-key", key);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn event(xp: i32) -> Value {
    json!({"lesson_id":"greetings","exercise_id":"exercise-1","event_type":"exercise_completed","xp_awarded":xp})
}

#[tokio::test]
async fn profiles_enforce_privacy_authorization_and_partial_updates() {
    let pool = database().await;
    let id = seed(&pool).await;
    let app = app(pool.clone());
    let path = format!("/v1/learners/{id}");
    let own = format!("learner:{id}");
    let (status, profile) = request(&app, "GET", &path, Some(&own), None, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        profile,
        json!({"id":id,"external_id":id.to_string(),"display_name":"Alice","cefr_level":"B1","total_xp":0,"created_at":profile["created_at"]})
    );
    assert_eq!(
        request(&app, "GET", &path, None, None, Value::Null).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &path,
            Some(&format!("learner:{}", Uuid::new_v4())),
            None,
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (a, b) = tokio::join!(
        request(
            &app,
            "PATCH",
            &path,
            Some(&own),
            None,
            json!({"display_name":"Changed"})
        ),
        request(
            &app,
            "PATCH",
            &path,
            Some(&own),
            None,
            json!({"cefr_level":"C2"})
        )
    );
    assert_eq!((a.0, b.0), (StatusCode::OK, StatusCode::OK));
    let profile = request(&app, "GET", &path, Some("service:test"), None, Value::Null)
        .await
        .1;
    assert_eq!(profile["display_name"], "Changed");
    assert_eq!(profile["cefr_level"], "C2");
    for body in [
        json!({"total_xp":9}),
        json!({"email":"bad"}),
        json!({"display_name":null}),
        json!({"cefr_level":"Z9"}),
        json!({"display_name":" "}),
    ] {
        assert_eq!(
            request(&app, "PATCH", &path, Some(&own), None, body)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    let missing = format!("/v1/learners/{}", Uuid::new_v4());
    for method in ["GET", "PATCH"] {
        assert_eq!(
            request(
                &app,
                method,
                &missing,
                Some("service:test"),
                None,
                json!({})
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn concurrent_retries_and_distinct_events_preserve_totals() {
    let pool = database().await;
    let id = seed(&pool).await;
    let app = app(pool.clone());
    let path = format!("/v1/learners/{id}/events");
    let mut jobs = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let app = app.clone();
        let path = path.clone();
        jobs.spawn(async move {
            request(
                &app,
                "POST",
                &path,
                Some("service:test"),
                Some("same-key"),
                event(10),
            )
            .await
        });
    }
    let mut created = 0;
    let mut ids = Vec::new();
    while let Some(result) = jobs.join_next().await {
        let (status, body) = result.unwrap();
        assert!(status == StatusCode::OK || status == StatusCode::CREATED);
        created += usize::from(status == StatusCode::CREATED);
        ids.push(body["id"].clone());
    }
    assert_eq!(created, 1);
    assert!(ids.iter().all(|id| *id == ids[0]));
    assert_eq!(
        request(
            &app,
            "POST",
            &path,
            Some("service:test"),
            Some("same-key"),
            event(999)
        )
        .await
        .0,
        StatusCode::OK
    );
    for i in 0..12 {
        let app = app.clone();
        let path = path.clone();
        jobs.spawn(async move {
            request(
                &app,
                "POST",
                &path,
                Some("service:test"),
                Some(&format!("distinct-{i}")),
                event(3),
            )
            .await
        });
    }
    while let Some(result) = jobs.join_next().await {
        assert_eq!(result.unwrap().0, StatusCode::CREATED);
    }
    let learner = learner::Entity::find_by_id(id)
        .one(&pool)
        .await
        .unwrap()
        .unwrap();
    let progress = lesson_progress::Entity::find_by_id((id, "greetings".to_owned()))
        .one(&pool)
        .await
        .unwrap()
        .unwrap();
    let totals = (learner.total_xp, progress.completed_exercises, progress.xp);
    assert_eq!(totals, (46, 13, 46));
    let count = progress_event::Entity::find()
        .filter(progress_event::Column::LearnerId.eq(id))
        .count(&pool)
        .await
        .unwrap();
    assert_eq!(count, 13);
    let other = seed(&pool).await;
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("/v1/learners/{other}/events"),
            Some("service:test"),
            Some("same-key"),
            event(0)
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn failed_rollup_rolls_back_event_and_all_counters() {
    let pool = database().await;
    let id = seed(&pool).await;
    let original_learner = learner::Entity::find_by_id(id)
        .one(&pool)
        .await
        .unwrap()
        .unwrap();
    lesson_progress::ActiveModel {
        learner_id: Set(id),
        lesson_id: Set("greetings".into()),
        completed_exercises: Set(i32::MAX),
        xp: Set(0),
        updated_at: Set(Utc::now()),
    }
    .insert(&pool)
    .await
    .unwrap();
    let app = app(pool.clone());
    let path = format!("/v1/learners/{id}/events");
    let (status, body) = request(
        &app,
        "POST",
        &path,
        Some("service:test"),
        Some("rollback"),
        event(10),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"error":"internal server error"}));
    let learner = learner::Entity::find_by_id(id)
        .one(&pool)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(learner.total_xp, 0);
    assert_eq!(learner, original_learner);
    let progress = lesson_progress::Entity::find_by_id((id, "greetings".to_owned()))
        .one(&pool)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((progress.completed_exercises, progress.xp), (i32::MAX, 0));
    let count = progress_event::Entity::find()
        .filter(progress_event::Column::LearnerId.eq(id))
        .count(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    lesson_progress::Entity::update_many()
        .set(lesson_progress::ActiveModel {
            completed_exercises: Set(0),
            ..Default::default()
        })
        .filter(lesson_progress::Column::LearnerId.eq(id))
        .exec(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &path,
            Some("service:test"),
            Some("rollback"),
            event(10)
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn event_listing_preserves_filters_ordering_and_offsets() {
    let pool = database().await;
    let id = seed(&pool).await;
    let app = app(pool.clone());
    let path = format!("/v1/learners/{id}/events");
    for (i, xp) in [10, 20, 20].into_iter().enumerate() {
        assert_eq!(
            request(
                &app,
                "POST",
                &path,
                Some("service:test"),
                Some(&i.to_string()),
                event(xp)
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let (status, body) = request(
        &app,
        "GET",
        &format!("{path}?sort=xp_awarded&order=desc&limit=2"),
        Some("service:test"),
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let page = body.as_array().unwrap();
    assert_eq!(
        (
            page[0]["xp_awarded"].as_i64(),
            page[1]["xp_awarded"].as_i64()
        ),
        (Some(20), Some(20))
    );
    assert!(page[0]["id"].as_i64() > page[1]["id"].as_i64());
    let last = request(
        &app,
        "GET",
        &format!("{path}?sort=xp_awarded&order=desc&limit=2&offset=2"),
        Some("service:test"),
        None,
        Value::Null,
    )
    .await
    .1;
    assert_eq!(last[0]["xp_awarded"], 10);
    let offset = request(
        &app,
        "GET",
        &format!("{path}?sort=xp_awarded&order=desc&limit=2&offset=1"),
        Some("service:test"),
        None,
        Value::Null,
    )
    .await;
    assert_eq!(offset, (StatusCode::OK, json!([page[1], last[0]])));
    let filtered = request(
        &app,
        "GET",
        &format!("{path}?event_type=unknown"),
        Some("service:test"),
        None,
        Value::Null,
    )
    .await;
    assert_eq!(filtered, (StatusCode::OK, json!([])));
    // Force timestamp ties so every ordering variant must use its ID tie-break.
    progress_event::Entity::update_many()
        .set(progress_event::ActiveModel {
            created_at: Set("2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap()),
            ..Default::default()
        })
        .filter(progress_event::Column::LearnerId.eq(id))
        .exec(&pool)
        .await
        .unwrap();
    for sort in ["created_at", "xp_awarded"] {
        for order in ["asc", "desc"] {
            for filter in ["", "&event_type=exercise_completed"] {
                let (status, rows) = request(
                    &app,
                    "GET",
                    &format!("{path}?sort={sort}&order={order}{filter}"),
                    Some("service:test"),
                    None,
                    Value::Null,
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                let rows = rows.as_array().unwrap();
                assert_eq!(rows.len(), 3);
                for pair in rows.windows(2) {
                    let left = (
                        if sort == "xp_awarded" {
                            pair[0]["xp_awarded"].as_i64().unwrap()
                        } else {
                            0
                        },
                        pair[0]["id"].as_i64().unwrap(),
                    );
                    let right = (
                        if sort == "xp_awarded" {
                            pair[1]["xp_awarded"].as_i64().unwrap()
                        } else {
                            0
                        },
                        pair[1]["id"].as_i64().unwrap(),
                    );
                    assert!(if order == "asc" {
                        left < right
                    } else {
                        left > right
                    });
                }
            }
        }
    }
}

#[tokio::test]
async fn event_endpoints_enforce_authorization_and_missing_learners() {
    let pool = database().await;
    let id = seed(&pool).await;
    let app = app(pool);
    let path = format!("/v1/learners/{id}/events");
    assert_eq!(
        request(&app, "POST", &path, None, Some("unauthorized"), event(1))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let foreign = format!("learner:{}", Uuid::new_v4());
    for method in ["GET", "POST"] {
        assert_eq!(
            request(
                &app,
                method,
                &path,
                Some(&foreign),
                Some("foreign"),
                event(1)
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(
                &app,
                method,
                &format!("/v1/learners/{}/events", Uuid::new_v4()),
                Some("service:test"),
                Some("missing"),
                event(1)
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn sqlite_enforces_foreign_keys_and_cascades_learner_deletion() {
    let pool = database().await;
    let orphan = lesson_progress::ActiveModel {
        learner_id: Set(Uuid::new_v4()),
        lesson_id: Set("orphan".into()),
        completed_exercises: Set(0),
        xp: Set(0),
        updated_at: Set(Utc::now()),
    }
    .insert(&pool)
    .await;
    assert!(orphan.is_err(), "SQLite must enforce foreign keys");

    let id = seed(&pool).await;
    let app = app(pool.clone());
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("/v1/learners/{id}/events"),
            Some("service:test"),
            Some("cascade"),
            event(10)
        )
        .await
        .0,
        StatusCode::CREATED
    );
    learner::Entity::delete_by_id(id).exec(&pool).await.unwrap();
    assert_eq!(
        lesson_progress::Entity::find()
            .filter(lesson_progress::Column::LearnerId.eq(id))
            .count(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        progress_event::Entity::find()
            .filter(progress_event::Column::LearnerId.eq(id))
            .count(&pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn xp_overflow_rolls_back_on_sqlite_instead_of_promoting_to_real() {
    for overflow_learner in [true, false] {
        let pool = database().await;
        let id = seed(&pool).await;
        learner::Entity::update_many()
            .set(learner::ActiveModel {
                total_xp: Set(if overflow_learner { i64::MAX } else { 0 }),
                ..Default::default()
            })
            .filter(learner::Column::Id.eq(id))
            .exec(&pool)
            .await
            .unwrap();
        lesson_progress::ActiveModel {
            learner_id: Set(id),
            lesson_id: Set("greetings".into()),
            completed_exercises: Set(1),
            xp: Set(if overflow_learner { 0 } else { i64::MAX }),
            updated_at: Set(Utc::now()),
        }
        .insert(&pool)
        .await
        .unwrap();
        let before = learner::Entity::find_by_id(id)
            .one(&pool)
            .await
            .unwrap()
            .unwrap();
        let progress_before = lesson_progress::Entity::find_by_id((id, "greetings".to_owned()))
            .one(&pool)
            .await
            .unwrap()
            .unwrap();
        let app = app(pool.clone());
        let response = request(
            &app,
            "POST",
            &format!("/v1/learners/{id}/events"),
            Some("service:test"),
            Some("xp-overflow"),
            event(1),
        )
        .await;
        assert_eq!(response.0, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            learner::Entity::find_by_id(id)
                .one(&pool)
                .await
                .unwrap()
                .unwrap(),
            before
        );
        assert_eq!(
            lesson_progress::Entity::find_by_id((id, "greetings".to_owned()))
                .one(&pool)
                .await
                .unwrap()
                .unwrap(),
            progress_before
        );
        assert_eq!(
            progress_event::Entity::find().count(&pool).await.unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn database_checks_and_idempotency_uniqueness_reject_invalid_writes() {
    let pool = database().await;
    let id = seed(&pool).await;
    for invalid in [
        learner::ActiveModel {
            cefr_level: Set("Z9".into()),
            ..Default::default()
        },
        learner::ActiveModel {
            total_xp: Set(-1),
            ..Default::default()
        },
    ] {
        assert!(
            learner::Entity::update_many()
                .set(invalid)
                .filter(learner::Column::Id.eq(id))
                .exec(&pool)
                .await
                .is_err()
        );
    }
    for (completed, xp) in [(-1, 0), (0, -1)] {
        assert!(
            lesson_progress::ActiveModel {
                learner_id: Set(id),
                lesson_id: Set("invalid-progress".into()),
                completed_exercises: Set(completed),
                xp: Set(xp),
                updated_at: Set(Utc::now()),
            }
            .insert(&pool)
            .await
            .is_err()
        );
    }
    let app = app(pool.clone());
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("/v1/learners/{id}/events"),
            Some("service:test"),
            Some("unique-key"),
            event(10)
        )
        .await
        .0,
        StatusCode::CREATED
    );
    for (key, xp) in [("negative-xp", -1), ("unique-key", 10)] {
        assert!(
            progress_event::ActiveModel {
                learner_id: Set(id),
                lesson_id: Set("greetings".into()),
                exercise_id: Set("exercise-1".into()),
                event_type: Set("exercise_completed".into()),
                xp_awarded: Set(xp),
                idempotency_key: Set(key.into()),
                created_at: Set(Utc::now()),
                ..Default::default()
            }
            .insert(&pool)
            .await
            .is_err()
        );
    }
    assert_eq!(
        learner::Entity::find_by_id(id)
            .one(&pool)
            .await
            .unwrap()
            .unwrap()
            .total_xp,
        10
    );
    assert_eq!(
        progress_event::Entity::find().count(&pool).await.unwrap(),
        1
    );
}

#[tokio::test]
async fn validated_query_fields_preserve_bounds_and_configured_page_cap() {
    let pool = database().await;
    let id = seed(&pool).await;
    let regular = app(pool.clone());
    let restricted = app_with_page_limit(pool, 2);
    let path = format!("/v1/learners/{id}/events");
    for query in [
        "limit=0",
        "limit=101",
        "limit=-1",
        "offset=-1",
        "offset=10001",
        "sort=email",
        "order=sideways",
        "limit=oops",
        "event_type=",
        "event_type=%20%20",
        "event_type=private%00value",
        "limit=4294967296",
        "offset=4294967296",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([format!("event_type={}", "%C3%A9".repeat(101))])
    {
        assert_eq!(
            request(
                &regular,
                "GET",
                &format!("{path}?{query}"),
                Some("service:test"),
                None,
                Value::Null
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for query in [
        "limit=100&offset=10000".to_owned(),
        format!("event_type={}", "%C3%A9".repeat(100)),
    ] {
        assert_eq!(
            request(
                &regular,
                "GET",
                &format!("{path}?{query}"),
                Some("service:test"),
                None,
                Value::Null
            )
            .await,
            (StatusCode::OK, json!([]))
        );
    }
    assert_eq!(
        request(
            &restricted,
            "GET",
            &format!("{path}?limit=2"),
            Some("service:test"),
            None,
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &restricted,
            "GET",
            &format!("{path}?limit=3"),
            Some("service:test"),
            None,
            Value::Null
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn validated_body_fields_reject_invalid_values_without_sanitizing_valid_input() {
    let pool = database().await;
    let id = seed(&pool).await;
    let app = app(pool);
    let profile_path = format!("/v1/learners/{id}");
    let event_path = format!("{profile_path}/events");
    for body in [
        event(-1),
        json!({"lesson_id":"","exercise_id":"x","event_type":"exercise_completed","xp_awarded":1}),
        json!({"lesson_id":"x","exercise_id":"x","event_type":"other","xp_awarded":1}),
    ] {
        assert_eq!(
            request(
                &app,
                "POST",
                &event_path,
                Some("service:test"),
                Some("invalid"),
                body
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for name in ["é".repeat(201), "private\0name".into()] {
        assert_eq!(
            request(
                &app,
                "PATCH",
                &profile_path,
                Some("service:test"),
                None,
                json!({"display_name": name})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let name = format!(" {} ", "é".repeat(198));
    let (status, profile) = request(
        &app,
        "PATCH",
        &profile_path,
        Some("service:test"),
        None,
        json!({"display_name": name}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(profile["display_name"], name);

    for field in ["lesson_id", "exercise_id"] {
        for value in [" ".into(), "id\0value".into(), "é".repeat(201)] {
            let mut body = event(1);
            body[field] = json!(value);
            assert_eq!(
                request(
                    &app,
                    "POST",
                    &event_path,
                    Some("service:test"),
                    Some("invalid-input"),
                    body
                )
                .await
                .0,
                StatusCode::BAD_REQUEST
            );
        }
    }
    let mut out_of_range = event(1);
    out_of_range["xp_awarded"] = json!(i64::from(i32::MAX) + 1);
    assert_eq!(
        request(
            &app,
            "POST",
            &event_path,
            Some("service:test"),
            Some("invalid-input"),
            out_of_range
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let id_text = "é".repeat(200);
    let body = json!({"lesson_id": id_text, "exercise_id": id_text,
        "event_type":"exercise_completed","xp_awarded":0});
    let (status, created) = request(
        &app,
        "POST",
        &event_path,
        Some("service:test"),
        Some("valid-boundary"),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["lesson_id"], id_text);
    assert_eq!(created["exercise_id"], id_text);
    assert_eq!(created["xp_awarded"], 0);
    let (status, progress) = request(
        &app,
        "GET",
        &format!("{profile_path}/lessons/{}", "%C3%A9".repeat(200)),
        Some("service:test"),
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(progress["lesson_id"], id_text);
    assert_eq!(progress["completed_exercises"], 1);
    for lesson in ["%20".into(), "private%00value".into(), "x".repeat(201)] {
        assert_eq!(
            request(
                &app,
                "GET",
                &format!("{profile_path}/lessons/{lesson}"),
                Some("service:test"),
                None,
                Value::Null
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn validated_idempotency_keys_enforce_header_content_length_and_uniqueness() {
    let pool = database().await;
    let id = seed(&pool).await;
    let app = app(pool);
    let path = format!("/v1/learners/{id}/events");
    assert_eq!(
        request(&app, "POST", &path, Some("service:test"), None, event(1))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    for key in ["".into(), "   ".into(), "\t".into(), "k".repeat(201)] {
        assert_eq!(
            request(
                &app,
                "POST",
                &path,
                Some("service:test"),
                Some(&key),
                event(1)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let duplicate = Request::builder()
        .method("POST")
        .uri(&path)
        .header("content-type", "application/json")
        .header("authorization", "Bearer service:test")
        .header("idempotency-key", "first")
        .header("idempotency-key", "second")
        .body(Body::from(event(1).to_string()))
        .unwrap();
    let response = app.clone().oneshot(duplicate).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let (status, replay) = request(
        &app,
        "POST",
        &path,
        Some("service:test"),
        Some("first"),
        event(99),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, created);
    let key = "k".repeat(200);
    let (status, created) = request(
        &app,
        "POST",
        &path,
        Some("service:test"),
        Some(&key),
        event(1),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, replay) = request(
        &app,
        "POST",
        &path,
        Some("service:test"),
        Some(&key),
        event(99),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created, replay);
}
