//! Service configuration, loaded from the environment.

use std::{env, fmt};

/// A string whose contents are redacted in `Debug`/`Display` output.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(\"***redacted***\")")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***redacted***")
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: SecretString,
    pub bind_addr: String,
    pub content_service_api_key: SecretString,
    pub max_page_size: i64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: SecretString::new(
                env::var("DATABASE_URL")
                    .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?,
            ),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            content_service_api_key: SecretString::new(
                env::var("CONTENT_SERVICE_API_KEY").unwrap_or_default(),
            ),
            max_page_size: 100,
        })
    }
}
