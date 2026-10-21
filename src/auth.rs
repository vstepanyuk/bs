//! Caller authentication.
//!
//! The API gateway validates a JWT and forwards the verified identity.
//! In this service the bearer token carries it directly:
//!
//!   Authorization: Bearer learner:<uuid>     -> Caller::Learner(uuid)
//!   Authorization: Bearer service:<any>      -> Caller::Service

use std::str::FromStr;

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    /// An end-user session scoped to one learner.
    Learner(Uuid),
    /// A trusted backend/admin caller.
    Service,
}

impl Caller {
    /// Authorize this caller to act on `learner_id`: a learner session may only
    /// act on its own record; a service caller may act on any.
    pub fn require_learner_access(&self, learner_id: Uuid) -> Result<(), AppError> {
        match self {
            Caller::Service => Ok(()),
            Caller::Learner(id) if *id == learner_id => Ok(()),
            Caller::Learner(_) => Err(AppError::Forbidden),
        }
    }
}

/// Parse the gateway's learner/service token format without exposing token values in errors.
impl FromStr for Caller {
    type Err = AppError;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        let (kind, rest) = token.split_once(':').ok_or(AppError::Unauthorized)?;
        match kind {
            "learner" => rest
                .parse::<Uuid>()
                .map(Caller::Learner)
                .map_err(|_| AppError::Unauthorized),
            "service" if !rest.is_empty() => Ok(Caller::Service),
            _ => Err(AppError::Unauthorized),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|header| header.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;

        token.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_learner_identity_and_nonempty_service_subjects() {
        let id = Uuid::from_u128(1);
        assert_eq!(
            format!("learner:{id}").parse::<Caller>().unwrap(),
            Caller::Learner(id)
        );
        for token in ["service:backend", "service:namespace:backend"] {
            assert_eq!(token.parse::<Caller>().unwrap(), Caller::Service);
        }
    }

    #[test]
    fn rejects_missing_subjects_invalid_uuids_and_unknown_token_kinds() {
        for token in [
            "",
            "learner",
            "learner:",
            "learner:invalid",
            "service:",
            "admin:backend",
        ] {
            assert!(
                matches!(token.parse::<Caller>(), Err(AppError::Unauthorized)),
                "{token}"
            );
        }
    }
}
