//! Agenterra Core Library
//!
//! This library provides the core functionality for generating AI agent
//! server code from OpenAPI specifications.

pub mod config;
pub mod error;
pub mod openapi;
pub mod utils;

pub use crate::{
    config::Config,
    error::{Error, Result},
    openapi::OpenApiContext,
};

/// Result type for Agenterra generation operations
pub type AgenterraResult<T> = std::result::Result<T, Error>;
