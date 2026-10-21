//! learner-progress-service.
//!
//! Tracks a learner's progress through lessons: records completed-exercise
//! events, rolls them up into per-lesson and per-learner XP counters, and reads
//! them back. Backed by Postgres.

pub mod auth;
pub mod config;
pub mod db;
pub mod dto;
pub mod entities;
pub mod error;
pub mod migration;
pub mod repos;
pub mod routes;

use std::sync::Arc;

use axum::Router;
use sea_orm::DatabaseConnection;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
        }
    }
}

/// Build the application router.
pub fn build_router(state: AppState) -> Router {
    routes::router(state)
}
