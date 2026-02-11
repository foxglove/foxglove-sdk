//! Serde types for the remote data loader manifest JSON.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_constant::ConstBool;
use serde_with::{base64::Base64, serde_as};
use std::num::NonZeroU16;

/// Manifest of upstream sources.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Human-readable display name for this manifest.
    #[serde(default)]
    pub name: Option<String>,
    /// Data sources in this manifest.
    pub sources: Vec<UpstreamSource>,
}

/// A data source from an upstream manifest.
///
/// Sources can be either static files (supporting range requests for random access)
/// or streamed sources that must be read sequentially.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", untagged)]
pub enum UpstreamSource {
    /// A static file that supports HTTP range requests.
    StaticFile {
        /// URL to fetch the file from.
        url: String,
        /// Marker indicating range request support (always true).
        #[allow(unused)]
        support_range_requests: ConstBool<true>,
    },
    /// A streamed source that must be read sequentially.
    Streamed(StreamedSource),
}

/// A URL data source which does not support range requests.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamedSource {
    /// URL to fetch the data from. Can be absolute or relative.
    /// If `id` is absent, this must uniquely identify the data.
    pub url: String,
    /// Identifier for the data source. If present, this must be unique.
    #[serde(default)]
    pub id: Option<String>,
    /// Topics present in the data.
    pub topics: Vec<Topic>,
    /// Schemas present in the data.
    pub schemas: Vec<Schema>,
    /// Earliest timestamp of any message in the data.
    pub start_time: DateTime<Utc>,
    /// Latest timestamp of any message in the data.
    pub end_time: DateTime<Utc>,
}

/// A topic in a streamed source.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    /// Topic name.
    pub name: String,
    /// Message encoding (e.g. `"protobuf"`).
    pub message_encoding: String,
    /// Schema ID, if this topic has an associated schema.
    pub schema_id: Option<NonZeroU16>,
}

/// A schema in a streamed source.
#[serde_as]
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    /// Unique schema ID within this source.
    pub id: NonZeroU16,
    /// Schema name.
    pub name: String,
    /// Schema encoding (e.g. `"protobuf"`).
    pub encoding: String,
    /// Raw schema data, serialized as base64.
    #[serde_as(as = "Base64")]
    pub data: Box<[u8]>,
}
