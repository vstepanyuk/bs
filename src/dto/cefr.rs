//! CEFR proficiency levels.

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// CEFR proficiency level (A1–C2).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[strum(parse_err_ty = AppError, parse_err_fn = invalid_cefr_level)]
pub enum CefrLevel {
    A1,
    A2,
    B1,
    B2,
    C1,
    C2,
}

fn invalid_cefr_level(_: &str) -> AppError {
    AppError::validation("cefr_level must be A1, A2, B1, B2, C1, or C2")
}
