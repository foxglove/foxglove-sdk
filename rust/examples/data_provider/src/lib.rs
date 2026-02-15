//! Library entry point for the data provider example.
//!
//! This re-exports [`app()`] so integration tests can construct the router without duplicating
//! code.

mod server;
pub use server::app;
