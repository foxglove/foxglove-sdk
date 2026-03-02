//! Internal API client for the live visualization feature.
//!
//! This module is intended for internal use only and is subject to breaking changes at any time.
//! Do not depend on the stability of any types or functions in this module.

#![allow(unused)]

mod client;
#[cfg(test)]
pub(crate) mod test_utils;
mod types;

pub(crate) use client::{
    DeviceToken, FoxgloveApiClient, FoxgloveApiClientBuilder, FoxgloveApiClientError,
};
pub(crate) use types::{DeviceResponse, RtcCredentials};
