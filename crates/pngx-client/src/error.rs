use thiserror::Error;

/// Errors returned by the Paperless-ngx API client.
#[derive(Debug, Error)]
pub enum ApiError {
    /// The API token is invalid or missing.
    #[error("unauthorized: invalid or missing API token")]
    Unauthorized,

    /// The requested resource was not found.
    #[error("not found")]
    NotFound,

    /// The base URL could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// An I/O error occurred during a file operation.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A network-level error occurred.
    #[error("network error")]
    Network(#[source] Box<ureq::Error>),

    /// The request timed out.
    #[error("request timed out")]
    Timeout,

    /// The response body could not be deserialized as JSON.
    #[error("failed to deserialize response: {0}")]
    Deserialization(#[from] serde_json::Error),

    /// The server returned a pagination URL with a different scheme than
    /// the configured base URL, typically `http` instead of `https`. This
    /// usually means the server is behind a reverse proxy that terminates TLS
    /// but the server is not configured to trust forwarded headers (e.g.
    /// `X-Forwarded-Proto`).
    #[error(
        "server returned pagination URL with scheme \"{returned}\" but client uses \"{expected}\"; \
        configure your server to trust proxy headers (e.g. PAPERLESS_PROXY_SSL_HEADER)"
    )]
    SchemeMismatch {
        /// Scheme the client is configured with.
        expected: String,
        /// Scheme the server returned.
        returned: String,
    },

    /// The server returned an unexpected status code.
    #[error("server error ({status}): {message}")]
    Server {
        /// HTTP status code.
        status: u16,
        /// Error message from the server.
        message: String,
    },
}

impl From<ureq::Error> for ApiError {
    fn from(err: ureq::Error) -> Self {
        match err {
            ureq::Error::StatusCode(status) => match status {
                401 | 403 => ApiError::Unauthorized,
                404 => ApiError::NotFound,
                _ => ApiError::Server {
                    status,
                    message: "unexpected status code".to_string(),
                },
            },
            ureq::Error::Timeout(_) => ApiError::Timeout,
            other => ApiError::Network(Box::new(other)),
        }
    }
}
