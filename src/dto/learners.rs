//! Public learner profiles and their mutable fields.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::{cefr::CefrLevel, inputs::DisplayName};

/// A learner profile containing no private identity fields.
#[derive(Serialize, sea_orm::FromQueryResult, utoipa::ToSchema)]
pub struct LearnerProfile {
    pub id: Uuid,
    pub external_id: String,
    pub display_name: String,
    #[schema(value_type = crate::dto::cefr::CefrLevel)]
    pub cefr_level: String,
    pub total_xp: i64,
    pub created_at: DateTime<Utc>,
}

/// Omitted fields remain unchanged; explicit nulls and unknown fields are rejected.
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchLearner {
    #[serde(default, deserialize_with = "non_null")]
    /// When supplied, must be nonblank and contain no NUL characters.
    #[schema(value_type = Option<String>, nullable = false, min_length = 1, max_length = 200)]
    pub(crate) display_name: Option<DisplayName>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub(crate) cefr_level: Option<CefrLevel>,
}

fn non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
