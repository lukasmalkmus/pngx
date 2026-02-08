//! A Rust client library for the [Paperless-ngx](https://docs.paperless-ngx.com) REST API.
//!
//! # Example
//!
//! ```no_run
//! use pngx_client::Client;
//!
//! let client = Client::new("https://paperless.example.com", "your-api-token")?;
//! let response = client.documents()?;
//! for doc in &response.results {
//!     println!("{}: {}", doc.id, doc.title);
//! }
//! # Ok::<(), pngx_client::ApiError>(())
//! ```

#![warn(missing_docs)]

mod client;
mod error;
mod types;

pub use jiff;

pub use client::{Client, ClientBuilder};
pub use error::ApiError;
pub use types::{
    Correspondent, Document, DocumentType, DocumentVersion, PaginatedResponse, Tag, UiSettings,
};
