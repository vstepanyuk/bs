# Learner Progress Service

An axum + SeaORM 2/Postgres service with authenticated learner profiles, partial
profile updates, idempotent exercise completion, and paginated event history.
See [`../API_SPEC.md`](../API_SPEC.md) for endpoint contracts and
[`../REPORT.md`](../REPORT.md) for design decisions and operational notes.
[`../openapi.yaml`](../openapi.yaml) is generated from the handler annotations
registered through `utoipa-axum`. The same contract is served at `/openapi.json`.

## Change from the original starter

The starter used SQLx directly. I switched application data access to SeaORM 2
to express queries through typed entities and query builders, and to keep schema
migrations in Rust. This also lets integration tests run against isolated SQLite
databases without a Postgres server, while keeping Postgres for deployment.
The tradeoff is an ORM dependency and less direct SQL control; transactions,
locking, checked counters, and public-column projections remain explicit.
SeaORM still uses SQLx internally as its database driver.

I also introduced `nutype` wrappers for validated request fields. String length,
nonblank/NUL checks, and nonnegative XP are enforced when values are constructed
or deserialized, so repositories receive validated types rather than unchecked
primitives. Idempotency keys use the same approach during header extraction.
Values are validated without trimming or changing client input; authorization,
configured pagination limits, and counter-overflow checks remain explicit.

I use `utoipa` and `utoipa-axum` to generate OpenAPI documentation from handler
annotations and DTO schemas rather than maintain a separate handwritten spec.
The generated contract is served at `/openapi.json`, exported to `openapi.yaml`,
and displayed through the opt-in Swagger UI. These annotations document request
constraints; runtime validation remains in the request types and handlers.

## Prerequisites

- Current stable Rust (edition 2024; verified with rustc 1.99.1)
- Docker (for Postgres), or any local Postgres 16

## Run it

```bash
# 1. Start Postgres
docker compose up -d db

# 2. Configure the environment
cp .env.example .env

# 3. Run. Migrations apply automatically; dev-seed plus SEED_DEV_DATA=1
#    inserts two demo learners (Alice, Bruno).
set -a && source .env && set +a
cargo run --features dev-seed
```

The service listens on `:8080`. Versioned SeaORM migrations live in `src/migration/`
and run on boot. Schema and data access use entity/query builders, not SQL strings.
Development seeding requires both the opt-in `dev-seed` Cargo feature and
`SEED_DEV_DATA=1`. Without that feature the seed function and startup call are
not compiled; setting the environment variable alone has no effect.

## Try the service

```bash
# Liveness
curl -s localhost:8080/healthz

# Reference endpoint: Bruno's seeded lesson progress.
# Auth is a bearer token "learner:<uuid>" (a stand-in for a verified JWT sub).
# A learner token may only access its own learner_id. See src/auth.rs.
BRUNO=22222222-2222-2222-2222-222222222222
curl -s -H "Authorization: Bearer learner:$BRUNO" \
  localhost:8080/v1/learners/$BRUNO/lessons/lesson-greetings
```

```bash
curl -s -H "Authorization: Bearer learner:$BRUNO" \
  localhost:8080/v1/learners/$BRUNO
curl -s -X PATCH -H "Authorization: Bearer learner:$BRUNO" \
  -H 'Content-Type: application/json' -d '{"display_name":"Bruno B."}' \
  localhost:8080/v1/learners/$BRUNO
curl -s -X POST -H "Authorization: Bearer learner:$BRUNO" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: demo-exercise-1' \
  -d '{"lesson_id":"lesson-greetings","exercise_id":"ex-1","event_type":"exercise_completed","xp_awarded":10}' \
  localhost:8080/v1/learners/$BRUNO/events
curl -s -H "Authorization: Bearer learner:$BRUNO" \
  "localhost:8080/v1/learners/$BRUNO/events?limit=20&sort=created_at&order=desc"
```

Repeating a POST with the same learner/key returns the original event with 200,
even if the valid payload differs. New events return 201.
PATCH accepts only `display_name` (nonblank, at most 200 Unicode characters) and
`cefr_level` (A1–C2); null and unknown fields are rejected, `{}` is accepted.
Lesson/exercise identifiers (including lesson lookup paths) are nonblank, contain
no NUL characters, and have at most 200 Unicode characters.
Only `exercise_completed` and nonnegative 32-bit XP are accepted.
Idempotency keys are nonblank printable ASCII, at most 200 bytes.
If multiple `Idempotency-Key` headers are supplied, the first value is used.
Lists use limit 1–100 and offset 0–10,000; invalid pagination/sort/order returns 400.
Event-type filters are nonblank, at most 100 characters. Ties sort by event ID in
the same direction. Missing learners return 404, including for event lists.
Bodies are capped at 16 KiB. Malformed JSON/query input returns a fixed 400 response.
`dto/inputs.rs` uses nutype to validate request fields during deserialization
and idempotency keys during header extraction. Values are not trimmed or otherwise
normalized. CEFR and sorting remain enums; configured page-size caps and counter
overflow checks remain runtime validations.

Event writes and profile patches use SeaORM's native async transaction callbacks:
success commits, while application failures roll back. Event retries acquire the
learner lock and return the original public event before reading rollup counters.
Lesson progress is persisted with an upsert of counters already checked in Rust;
the learner lock and transaction preserve serialized, atomic rollups.
New events consume validated identifiers without copying, except for the lesson ID
shared by the progress upsert and the event response.

## Layout

```
src/
  main.rs          binary entrypoint (env config, migrate, seed, serve)
  lib.rs           AppState + router builder
  config.rs        Config and the SecretString type
  error.rs         AppError and its HTTP mapping
  auth.rs          Caller extractor + authorization helper
  db.rs            connection pool, migrations, dev seed
  dto/             API types, request validation, and response DTOs
    cefr.rs        proficiency level and strum parsing
    inputs.rs      nutype field invariants and borrowed header validation
    learners.rs    public profile and partial-update input
    events.rs      event DTOs, input, and query parameters
    lessons.rs     lesson progress response
  repos/           SeaORM entity queries and transactions
    learners.rs    profile lookup/update and existence checks
    events.rs      idempotent event writes and bounded listing
    lessons.rs     lesson progress lookup
  routes/          one module per resource
  entities/        database models, keys, columns, and relations
  migration/       portable versioned SeaORM schema migrations
```

## Tests

Integration tests create an isolated in-memory SQLite database per test and apply
the same SeaORM migrations as the service. No Postgres server, Docker, environment
variables, or `CREATEDB` privilege is needed. Each in-memory database uses one
connection so concurrent requests share the same database.

```bash
cargo test --locked
cargo test --locked --features dev-docs
cargo build
cargo +nightly fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Formatting alone uses nightly rustfmt for merged per-crate imports and
standard-library/external/local import groups. Run `cargo +nightly fmt --all`
to apply formatting. Builds and tests do not require nightly.

SQLite tests do not prove Postgres row-lock behavior or integer semantics.
Counter arithmetic is explicitly checked on both backends. Validate concurrent
retries and distinct-event rollups against Postgres before production deployment.

The initial SeaORM migration uses `IF NOT EXISTS` to adopt an existing starter
Postgres schema without deleting learner data. Old SQLx migration metadata is
left untouched; only SeaORM's migration ledger tracks subsequent migrations.
Adoption assumes the original compatible schema, not an arbitrary database.
Legacy redundant indexes may remain on adopted databases; review them separately.

The initial migration's `down` drops `progress_events`, `lesson_progress`, then
`learners`, in foreign-key-safe order. **This deletes all application data,
including tables adopted from the original SQLx schema.** It does not restore
the previous SQLx schema or migration history. Invoke it through
`Migrator::down(&db, Some(1))` only for an intentional schema teardown.

For a database-free local API session:

```bash
DATABASE_URL=sqlite::memory: SEED_DEV_DATA=1 cargo run --features dev-docs,dev-seed
```

That database disappears when the process stops; production remains Postgres.
SeaORM uses SQLx internally as its driver, but application code has no direct
SQLx dependency or handwritten SQL queries.

## Export the API contract

From `starter/`, regenerate the checked-in contract after changing handlers:

```bash
cargo run --locked --example export_openapi > ../openapi.yaml
```

The exporter does not need a database. `strum::EnumString` derives CEFR's
`FromStr` implementation with a custom error function returning `AppError` directly.

## Development API explorer

```bash
cargo run --features dev-docs
```

Open <http://localhost:8080/docs/> for Swagger UI, backed by `/openapi.json`.
Use **Authorize** with `learner:<uuid>` or `service:<anything>` to try requests.
The UI is opt-in and compiled into debug builds only: `/docs` is absent from
normal builds and release builds, even with `--features dev-docs`.
Swagger UI assets are vendored; no CDN or build-time asset download is needed.
