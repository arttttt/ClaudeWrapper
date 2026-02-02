//! Error types for thinking transformation.

use thiserror::Error;

/// Errors that can occur during thinking block transformation.
#[derive(Debug, Error)]
pub enum TransformError {
    /// JSON parsing or serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Summarization service failed
    #[error("Summarization failed: {0}")]
    Summarization(#[from] SummarizeError),

    /// Backend not available for summarization
    #[error("Summarizer backend not available: {0}")]
    BackendUnavailable(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Errors from the summarizer client.
#[derive(Debug, Error)]
pub enum SummarizeError {
    /// API key not configured
    #[error("API key not configured: set api_key in [thinking.summarize] section")]
    NotConfigured,

    /// Network error during API call
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// API returned an error response
    #[error("API error (status {status}): {message}")]
    ApiError { status: u16, message: String },

    /// Failed to parse API response
    #[error("Failed to parse response: {0}")]
    ParseError(String),

    /// No content in response
    #[error("No content in API response")]
    EmptyResponse,
}
