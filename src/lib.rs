//! EternaLibre Notes — a server-side rendered, copyleft Markdown notes application.
//!
//! The crate is split into a library plus a thin binary so that the HTTP layer
//! can be exercised from integration tests in `tests/`.

pub mod auth;
pub mod markdown;
pub mod notes;
pub mod pages;
pub mod routes;
pub mod setup;
pub mod syntax;
pub mod themes;
